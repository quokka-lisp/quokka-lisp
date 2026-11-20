;;;; The prelude — the first Lisp Quokka runs.
;;;;
;;;; This file is compiled into the VM binary with `include_str!` and
;;;; interpreted at boot, after the primitives are installed and before any
;;;; user code. See `crates/quokka-vm/src/boot.rs`.
;;;;
;;;; Everything here could have been written in Rust, and was going to be.
;;;; Rule 1 says it should not be: the evaluator provides `if`, `let`, `block`
;;;; and `defmacro`, and everything built *out of* those belongs above the
;;;; line. This file is where that line first has something on the other side
;;;; of it.
;;;;
;;;; Three constraints apply to every form below.
;;;;
;;;; 1. **Tier 0 only** (rule 5). No `loop`, no `format`, no CLOS. This is the
;;;;    same restriction the compiler's own sources will be under, so the
;;;;    prelude is the first test of whether tier 0 is big enough to work in.
;;;;
;;;; 2. **Order matters.** There is no forward reference and no second pass:
;;;;    each form is evaluated as it is read, so a macro must be defined before
;;;;    anything that uses it — including other macros in this file. `cond`
;;;;    appears before `and`, because `and` is written using `cond`.
;;;;
;;;; 3. **Common Lisp semantics** (rule 6). These are CL's macros, not
;;;;    lookalikes. `tests/lisp/prelude.lisp` checks them against SBCL.

;;; ------------------------------------------------------------------
;;; Conditionals
;;; ------------------------------------------------------------------

;; The two-armed `if` is a special operator because it must be: it decides
;; which of its arguments is evaluated. Everything else in this section is
;; sugar over it, and sugar is exactly what belongs in Lisp.

(defmacro when (test &body body)
  `(if ,test (progn ,@body) nil))

(defmacro unless (test &body body)
  `(if ,test nil (progn ,@body)))

;; `cond` expands into nested `if`s, one clause at a time, by calling itself
;; on the clauses it has not reached yet. A macro that expands into a use of
;; itself is not a loop: each expansion is one `if` smaller than the last, and
;; the empty case stops it.
;;
;; The awkward part is a clause with no body. CLHS says `(cond (x))` returns
;; *x's value* when x is true, so the test cannot simply be evaluated twice —
;; `(cond ((pop queue)))` would pop twice. It is bound to a gensym instead,
;; which is the same trick `or` needs below, and for the same reason.
(defmacro cond (&rest clauses)
  (if (null clauses)
      nil
      (let ((clause (car clauses))
            (rest (cdr clauses)))
        (if (null (cdr clause))
            (let ((once (gensym)))
              `(let ((,once ,(car clause)))
                 (if ,once ,once (cond ,@rest))))
            `(if ,(car clause)
                 (progn ,@(cdr clause))
                 (cond ,@rest))))))

;;; ------------------------------------------------------------------
;;; Boolean connectives
;;; ------------------------------------------------------------------

;; `and` and `or` cannot be functions, for the reason every macro exists: a
;; function receives values, and `(or (safe-p x) (blow-up x))` must be able to
;; not call `blow-up` at all. Short-circuiting *is* control flow.
;;
;; Both return a useful value rather than just true or false. `(and 1 2)` is 2
;; and `(or nil 3)` is 3, which is why they are worth having over `if`.

(defmacro and (&rest forms)
  (cond ((null forms) t)
        ((null (cdr forms)) (car forms))
        (t `(if ,(car forms) (and ,@(cdr forms)) nil))))

;; `or` has to hold on to the value it tested, or it would evaluate the form
;; twice to return it. A gensym, because the caller may well have a variable
;; called `value` or `tmp` and this macro must not care.
(defmacro or (&rest forms)
  (cond ((null forms) nil)
        ((null (cdr forms)) (car forms))
        (t (let ((once (gensym)))
             `(let ((,once ,(car forms)))
                (if ,once ,once (or ,@(cdr forms))))))))

;;; ------------------------------------------------------------------
;;; defun
;;; ------------------------------------------------------------------

;; Until this point `defun` was a special operator written in Rust. It is a
;; macro in Common Lisp, and now it is one here — which is why it comes after
;; `cond`, `and` and `or`: it is written using them.
;;
;; CLHS says the body of a `defun` is wrapped in a **block named after the
;; function**, which is what makes
;;
;;     (defun find-it (x) ... (return-from find-it hit) ...)
;;
;; work. The Rust version did not do this and nobody noticed, because an
;; omission inside a Rust function is invisible. Here it is one word in an
;; expansion anyone can read.
;;
;; But a `block` is not free: it pushes a frame, and a frame outstanding at a
;; call is exactly what tail-call elimination (chapter 10) needs *not* to
;; exist. Wrapping every function body in a block would mean
;;
;;     (defun count-down (n) (if (= n 0) 'done (count-down (- n 1))))
;;
;; grows the frame stack by one frame per call, which is the property Quokka
;; decided to have and would quietly have lost.
;;
;; So the block is emitted only when something in the body actually returns
;; from it. Real compilers do this by flow analysis and delete the block as
;; dead; this does it with a tree walk at expansion time, which is cruder and
;; sound in the same direction: a body that mentions `return-from` gets a
;; block it may not need (one frame, no harm), and the walk never removes a
;; block that is used.
;;
;; **The known gap**: a *macro* in the body that expands into `(return-from
;; name ...)` is not visible to this walk, and would fail with "there is no
;; exit named NAME here". Nothing in Quokka's own sources does that. A real
;; fix is to expand the body first, which needs `macroexpand-all`, which needs
;; the full lambda-list machinery — later.

;; Defined without `defun`, because `defun` is what it is for. This is the
;; bootstrap in miniature: `%set-symbol-function` and `lambda` are the two
;; things `defun` expands into, so writing one function by hand costs nothing
;; and breaks the circle.
(%set-symbol-function
 '%mentions-return-from
 ;; Written with `if` and `consp` rather than `not`, because `not` is defined
 ;; further down this file and there is no second pass. The prelude is the
 ;; first place where "order matters" stops being advice and starts being a
 ;; boot failure — this exact line cost one, reported as
 ;; `prelude form 8 failed: the function NOT is undefined`.
 (lambda (name form)
   (if (consp form)
       (cond ((eq (car form) 'return-from)
              (if (consp (cdr form)) (eq (car (cdr form)) name) nil))
             (t (or (%mentions-return-from name (car form))
                    (%mentions-return-from name (cdr form)))))
       nil)))

(defmacro defun (name args &body body)
  `(progn
     (%set-symbol-function
      ',name
      (lambda ,args
        ,@(if (%mentions-return-from name body)
              (list (cons 'block (cons name body)))
              body)))
     ',name))

;;; ------------------------------------------------------------------
;;; A few functions
;;; ------------------------------------------------------------------

;; Not macros, and not primitives either. `cadr` in Rust would be a primitive
;; that could never be changed from Lisp; here it is three characters of Lisp
;; and costs the Rust budget nothing.

(defun caar (x) (car (car x)))
(defun cadr (x) (car (cdr x)))
(defun cdar (x) (cdr (car x)))
(defun cddr (x) (cdr (cdr x)))
(defun caddr (x) (car (cddr x)))

(defun first (x) (car x))
(defun rest (x) (cdr x))
(defun second (x) (cadr x))
(defun third (x) (caddr x))

(defun not (x) (null x))

;; `endp` is the standard's list-end test, and the one the compiler's own
;; traversals should use: it is `null` plus the promise that the thing being
;; walked was a proper list.
(defun endp (x) (null x))

;; The accumulator helpers are `labels` rather than globals, so the prelude
;; does not leave names lying around that nobody asked for. The recursive call
;; in each is in tail position (chapter 10), so these run in constant frame
;; stack however long the list is — which is why no iteration construct is
;; needed to write them, and why `tagbody` can stay unused here.
;;
;; These used to carry a note saying they were the list cases only and would
;; be replaced when sequences arrived. Sequences have arrived, so they
;; dispatch: a list is walked, a vector or string is indexed. Common Lisp
;; calls all three sequences and the differential tests hold us to it.

(defun length (sequence)
  (cond ((null sequence) 0)
        ((consp sequence)
         (labels ((walk (rest n)
                    (if (endp rest) n (walk (cdr rest) (+ n 1)))))
           (walk sequence 0)))
        (t (%sequence-length sequence))))

(defun reverse (sequence)
  (if (listp sequence)
      (labels ((walk (rest acc)
                 (if (endp rest) acc (walk (cdr rest) (cons (car rest) acc)))))
        (walk sequence nil))
      (let ((n (%sequence-length sequence)))
        (let ((out (%make-vector n nil)))
          (labels ((walk (i)
                     (if (= i n)
                         out
                         (progn (%set-aref out i (aref sequence (- (- n 1) i)))
                                (walk (+ i 1))))))
            (walk 0))))))

;; `(reverse "abc")` gives a vector of characters rather than a string, which
;; Common Lisp would not: it returns a sequence of the same type. Making that
;; right needs a string allocator exposed to Lisp, and `make-string` is not
;; here yet. Said rather than hidden; `tests/lisp/vectors.lisp` does not ask.

(defun listp (x) (if (null x) t (consp x)))

(defun elt (sequence index)
  (if (listp sequence) (nth index sequence) (aref sequence index)))

;; `svref` is `aref` restricted to simple vectors. Quokka has only simple
;; vectors, so for now it is the same function under a second name.
(defun svref (vector index) (aref vector index))

;; CLHS `make-array` takes a list of dimensions, an element type, a fill
;; pointer and several other things. This takes one dimension and an initial
;; element, which is what a tier 0 compiler needs; the rest can arrive when
;; something wants it.
(defun make-array (size &key (initial-element nil))
  (%make-vector size initial-element))

;; Characters are immediate values in Quokka, so identity and equality are the
;; same test. `char=` exists anyway, because CLHS says `eq` on characters is
;; not something a program may rely on and code written against the standard
;; will call this.
(defun char= (a b) (eq a b))

;; Comparing two strings is O(n) in their length, which makes it the first
;; *library* function with a rule 9 question rather than the first primitive.
;; It is Lisp, so the evaluator can stop in the middle of it — which is
;; exactly why the rule is about primitives and not about functions.
(defun string= (a b)
  (let ((n (length a)))
    (if (= n (length b))
        (labels ((walk (i)
                   (cond ((= i n) t)
                         ((char= (aref a i) (aref b i)) (walk (+ i 1)))
                         (t nil))))
          (walk 0))
        nil)))

;; `nth` really is list-only in the standard, so this one is complete.
;; Indexing past the end is nil rather than an error, because `(car nil)` is
;; nil — one of the three jobs nil does (chapter 11).
(defun nth (n list)
  (if (= n 0) (car list) (nth (- n 1) (cdr list))))

;; CLHS says `member` compares with `eql` unless told otherwise. Quokka has no
;; `eql` yet, and `eq` is the same thing for every type that currently exists:
;; fixnums are immediate values, so two equal fixnums are the identical word.
;; That stops being true the moment bignums or floats arrive (#16, #17), and
;; this line will have to change then.
(defun member (item list)
  (cond ((endp list) nil)
        ((eq item (car list)) list)
        (t (member item (cdr list)))))

;;; ------------------------------------------------------------------
;;; Seeing what a macro did
;;; ------------------------------------------------------------------

;; Up to here, a macro that misbehaved could only be debugged by reasoning
;; about code nobody had seen. `macroexpand-1` runs one expansion and hands
;; back the form, so the expansion becomes an ordinary value you can print,
;; pick apart, or feed to `eval`.
;;
;; It is written in Lisp because it can be: `%macro-function` fetches the
;; expander and `apply` calls it, and the *evaluator* does the calling. A Rust
;; primitive could not have done this — running a macro means running Lisp,
;; and a primitive that called the evaluator would put Lisp back on the Rust
;; stack, which rule 4 exists to prevent. The same wall that defaults in
;; lambda lists ran into, and the same way round it.

(defun macroexpand-1 (form)
  (if (consp form)
      (let ((head (car form)))
        (if (symbolp head)
            (let ((expander (%macro-function head)))
              (if expander
                  ;; The macro's parameters are the argument *forms*, which
                  ;; is exactly what `(cdr form)` is.
                  (apply expander (cdr form))
                  form))
            form))
      form))

;; Expand until the head is no longer a macro.
;;
;; **One divergence from the standard, on purpose.** Common Lisp's
;; `macroexpand` returns two values — the form, and whether anything was
;; expanded — and loops on the second. Quokka has no multiple values yet, so
;; this loops while the form keeps *changing*, compared with `eq`.
;;
;; The difference shows only for a macro that returns the identical form object
;; it was given. The standard says that counts as an expansion, so a conforming
;; implementation loops forever; this one stops. Diverging towards terminating
;; seemed the better way to be wrong, and it goes away when
;; `multiple-value-call` arrives.
;;
;; Like the standard's, this expands the *top* of the form only. Nothing
;; descends into subforms; that is `macroexpand-all`, which does not exist.
(defun macroexpand (form)
  (let ((once (macroexpand-1 form)))
    (if (eq once form)
        form
        (macroexpand once))))

;;; ------------------------------------------------------------------
;;; Hash tables
;;; ------------------------------------------------------------------

;; Keys are hashed by **content**, never by address, because the collector
;; moves objects and an address-based hash would be correct until the next
;; collection and silently wrong afterwards. `crates/quokka-vm/src/hash.rs`
;; and issue #22 have the reasoning and what it costs.
;;
;; In short: an `eql` table takes fixnums, characters and symbols; an `equal`
;; table also takes strings. Any other key is refused rather than hashed by
;; something unreliable.

(defun make-hash-table (&key (test 'eql))
  ;; CLHS also takes :size, :rehash-size and :rehash-threshold, all of which
  ;; are hints an implementation may ignore. This one ignores them by not
  ;; having them yet.
  ;;
  ;; The test must be the *symbol* `eq`, `eql` or `equal`. Common Lisp also
  ;; accepts the function — `:test #'equal` — and comparing function objects
  ;; is how an implementation would tell which one it got. Quokka has no
  ;; `equal` function to take `#'` of, so the question does not arise yet.
  (%make-hash-table test))

;; `gethash` returns two values in Common Lisp — the value, and whether the
;; key was there. There are no multiple values yet, so the optional default is
;; the whole of the story: it covers everything except telling a stored nil
;; from a missing key, which is what the second value exists for.
(defun gethash (key table &optional default)
  (%gethash key table default))

;; `(setf (gethash k table) v)` is how Common Lisp writes this, and `setf` is
;; tier 1. Until then the writer has a name of its own.
(defun %set-gethash-fn (key table value)
  (%set-gethash key table value))

;; Iteration. The primitive hands back the entries as a list of (key . value)
;; cells, because calling the function is the evaluator's job rather than a
;; primitive's — the same division as `macroexpand` and for the same reason.
(defun maphash (function table)
  (labels ((walk (entries)
             (if (endp entries)
                 nil
                 (progn (funcall function (caar entries) (cdar entries))
                        (walk (cdr entries))))))
    (walk (%hash-table-entries table))))

;; `hash-table-keys` and `hash-table-values` were here, and the oracle threw
;; them out: they are not ANSI Common Lisp — they come from Alexandria, which
;; is a library. Defining them in the standard namespace is what rule 6 says
;; not to do, and the differential tests noticed within a minute of a test
;; file calling them. `maphash` is the standard way, and is enough.

