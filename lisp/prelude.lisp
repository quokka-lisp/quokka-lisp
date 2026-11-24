;;;; The prelude — the first Lisp Quokka runs.
;;;;
;;;; This file is compiled into the VM binary with `include_str!` and
;;;; interpreted at boot, after the primitives are installed and before any
;;;; user code. See `crates/quokka-bootstrap/src/boot.rs`.
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
;;; Declarations
;;; ------------------------------------------------------------------

;;; `(declare ...)` (#62, part 5). In Common Lisp it is not a form at all but
;;; syntax at the start of a body, and nearly every declaration is advice an
;;; implementation may ignore (CLHS 3.3.1). A macro that expands to nil lets
;;; both evaluators meet one anywhere without stopping; the compiler reads the
;;; one kind that changes meaning, `special`, where it binds variables.
(defmacro declare (&rest specifiers)
  nil)

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

;; The body of a function named NAME: in a block named NAME if it returns
;; from one, with its leading declarations and documentation string in front
;; of the block, where they still belong to the function. Until #78 they went
;; inside the block, where a `special` declaration was an ordinary form and
;; declared nothing. Used by `defun` and by the compiler's `flet` and
;; `labels`, whose functions have the same implicit block (CLHS 3.1.2.1.2.4).
(%set-symbol-function
 '%block-body
 (lambda (name body)
   (if (%mentions-return-from name body)
       (%block-body-after name body nil)
       body)))

(%set-symbol-function
 '%block-body-after
 (lambda (name body before)
   (if (%declaration-or-documentation-p body)
       (%block-body-after name (cdr body) (cons (car body) before))
       (%prepend-reversed before (list (cons 'block (cons name body)))))))

;; A body starting with a declaration, or with a string that is not its
;; only form — which is a documentation string, not the value.
(%set-symbol-function
 '%declaration-or-documentation-p
 (lambda (body)
   (if (consp body)
       (if (consp (car body))
           (eq (car (car body)) 'declare)
           (if (stringp (car body)) (consp (cdr body)) nil))
       nil)))

(%set-symbol-function
 '%prepend-reversed
 (lambda (reversed tail)
   (if (consp reversed)
       (%prepend-reversed (cdr reversed) (cons (car reversed) tail))
       tail)))

;; `(defun name ...)`, and since #78 `(defun (setf name) ...)`: a setf
;; function, stored in the function cell of the symbol `%setf-function-symbol`
;; keeps for NAME, its block named NAME.
(defmacro defun (name args &body body)
  (if (consp name)
      `(progn
         (%set-symbol-function
          (%setf-function-symbol ',(car (cdr name)))
          (lambda ,args ,@(%block-body (car (cdr name)) body)))
         ',name)
      `(progn
         (%set-symbol-function
          ',name
          (lambda ,args ,@(%block-body name body)))
         ',name)))

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

;; `(setf foo)` names a function too (CLHS 5.1.2.9): what `setf` calls for a
;; place `(foo ...)` that has no other expansion, with the new value first.
;; Its definition is kept as CLISP keeps one (#78): in the function cell of
;; an uninterned symbol named `(SETF FOO)`, which FOO's property list holds
;; under `%setf-function`. So `fboundp`, `fmakunbound`, `flet` and the
;; compiler's `#'` treat a setf function as the symbol's, and need nothing of
;; their own. Rejected: a table from names to functions, which every one of
;; those would have had to search.
(defun %setf-function-name-p (name)
  (if (consp name)
      (if (eq (car name) 'setf)
          (if (consp (cdr name))
              (if (symbolp (car (cdr name))) (null (cdr (cdr name))) nil)
              nil)
          nil)
      nil))

(defun %plist-value (plist indicator)
  (if (consp plist)
      (if (eq (car plist) indicator)
          (car (cdr plist))
          (%plist-value (cdr (cdr plist)) indicator))
      nil))

;; The symbol holding `#'(setf NAME)`, or nil if there has never been one.
(defun %setf-function-symbol-if-any (name)
  (%plist-value (symbol-plist name) '%setf-function))

(defun %setf-function-symbol (name)
  (let ((found (%setf-function-symbol-if-any name)))
    (if found
        found
        (let ((symbol (make-symbol (%string-append "(SETF "
                                                   (%string-append (symbol-name name) ")")))))
          (%set-symbol-plist name (cons '%setf-function (cons symbol (symbol-plist name))))
          symbol))))

;; A function name as the symbol whose function cell holds it.
(defun %function-name-symbol (name)
  (if (%setf-function-name-p name) (%setf-function-symbol (car (cdr name))) name))

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
;; Common Lisp would not: it returns a sequence of the same type. The allocator
;; it was waiting for now exists — `%make-string`, just below — so this is a
;; small fix rather than a missing feature. Said rather than hidden;
;; `tests/lisp/vectors.lisp` does not ask.

;; A source literal is a constant string and refuses `%set-aref` (issue #26),
;; because sharing an object is only sound when nothing can write to it. So a
;; program that wants to write characters makes a string of its own, and these
;; are the two ways Common Lisp offers.
;;
;; CLHS leaves `make-string`'s fill character unspecified when
;; `:initial-element` is not given, exactly as `make-array`'s is. So
;; `tests/lisp/` must not ask what it is — the same rule, for the same reason.
(defun make-string (size &key (initial-element #\Space))
  (%make-string size initial-element))

(defun copy-seq (sequence)
  (if (listp sequence)
      (labels ((walk (rest acc)
                 (if (endp rest) (reverse acc) (walk (cdr rest) (cons (car rest) acc)))))
        (walk sequence nil))
      (let ((n (%sequence-length sequence)))
        (let ((out (if (stringp sequence)
                       (%make-string n #\Space)
                       (%make-vector n nil))))
          (labels ((walk (i)
                     (if (= i n)
                         out
                         (progn (%set-aref out i (aref sequence i))
                                (walk (+ i 1))))))
            (walk 0))))))

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
                  ;; An expander is a function of the form and the
                  ;; environment (#76); here the environment is the null
                  ;; one. Two values, as CLHS says: the expansion, and
                  ;; whether anything happened.
                  (values (funcall expander form nil) t)
                  (values form nil)))
            (values form nil)))
      (values form nil)))

;; Expand until the head is no longer a macro, and say whether anything
;; happened.
;;
;; This used to loop while the form kept *changing*, compared with `eq`,
;; because there were no multiple values to loop on — a documented divergence
;; that said it would go away when `multiple-value-call` arrived. It has.
;;
;; Like the standard's, this expands the *top* of the form only. Nothing
;; descends into subforms; that is `macroexpand-all`, which does not exist.
(defun macroexpand (form)
  (labels ((walk (f changed)
             (multiple-value-bind (once expanded) (macroexpand-1 f)
               (if expanded (walk once t) (values f changed)))))
    (walk form nil)))

;;; ------------------------------------------------------------------
;;; Hash tables
;;; ------------------------------------------------------------------

;; Keys are hashed by **content** where they have one — numbers,
;; characters, symbols, and strings in an `equal` table — and every other key
;; by its address. The collector moves objects, so a table holding an
;; address-hashed key is marked stale whenever it does, and rehashed on its
;; next use (#64). A cons in an `equal` table, and an `equalp` table, need
;; `equal` and `equalp` to compare keys, so the primitives hand them to the
;; library (`lisp/library/hash-tables.lisp`, #71); here, before the library
;; exists, they are refused. `crates/quokka-vm/src/hash.rs` and issues #22,
;; #64 and #71 have the reasoning.

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

;; A marker meaning "no entry", made once at boot. A gensym, so no program
;; can store it as a value and fool the test below.
(setq %hash-missing (gensym))

;; Two values, as CLHS says: the value, and whether the key was there. That
;; second value is the only way to tell a stored nil from a missing key.
(defun gethash (key table &optional default)
  (let ((found (%gethash key table %hash-missing)))
    (if (eq found %hash-missing)
        (values default nil)
        (values found t))))

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

;;; ------------------------------------------------------------------
;;; setf
;;; ------------------------------------------------------------------

;; `docs/design.md` puts `setf` in tier 1, and this is tier 0. It was pulled
;; forward for a reason that has nothing to do with convenience.
;;
;; Writing a vector element or a hash-table entry has no name in Common Lisp
;; other than `(setf (aref v i) x)` and `(setf (gethash k h) v)`. Without
;; `setf`, the only way to write either was `%set-aref` and `%set-gethash` —
;; Quokka-only names that SBCL has no counterpart for. So those behaviours
;; were not *different* from the oracle's, they were **inexpressible to it**,
;; and two of the three things group C added were unverifiable against the
;; standard (#18). That is a hole in the main defence against implementing
;; Lisp wrong (chapter 12), and twenty lines closes it.
;;
;; **This is not Common Lisp's `setf`.** The real one is extensible:
;; `define-setf-expander` and `defsetf` let any program teach it a new place,
;; and `defstruct` relies on that to make its accessors settable. This one
;; knows a fixed list of places, which is enough for tier 0. The extensible
;; one is tier 1, in `lisp/library/setf.lisp` (#52), and it is built on top
;; of this one rather than beside it.

;; Reader name -> writer name, for places the prelude does not know about.
;; `defstruct` fills it in. A hash table rather than an alist because a
;; program may define a great many structures and `setf` consults this on
;; every expansion.
(setq %setf-setters (make-hash-table))

(defun %define-setf-writer (reader writer)
  (setf (gethash reader %setf-setters) writer))

;; Called while a `setf` form is being expanded, so an unknown place is
;; reported then rather than at run time.
(defun %setf-place (place value)
  (or (%setf-known-place place value)
      (%setf-unknown-place place)))

;; The code that stores VALUE into PLACE, or nil for a place this table does
;; not know. Split out of `%setf-place` for the tier-1 library (#52): its
;; `setf` and `get-setf-expansion` both ask this function first, so a place
;; known here expands the same before and after the library loads — which
;; the fixpoint needs, since the rebuild compiles the compiler with the
;; library's `setf` (chapter 43).
(defun %setf-known-place (place value)
  (cond
    ;; A plain variable is just setq.
    ((symbolp place) (list 'setq place value))
    ((consp place)
     (let ((head (car place))
           (args (cdr place)))
       (cond
         ;; Every subscript, then the value: an array may have several
         ;; dimensions (#67). One subscript expands exactly as it always did.
         ((eq head 'aref) (cons '%set-aref (append args (list value))))
         ((eq head 'svref) (list '%set-aref (car args) (car (cdr args)) value))
         ;; A default, `(setf (gethash k table default) v)`, is evaluated
         ;; for its effects in its place, after the table and before the
         ;; value (CLHS 5.1.1.1; found by ansi-test, #71). Without one the
         ;; expansion is what it always was.
         ((eq head 'gethash)
          (list '%set-gethash (car args) (car (cdr args))
                (if (cdr (cdr args)) (list 'progn (car (cdr (cdr args))) value) value)))
         ((eq head 'symbol-function) (list '%set-symbol-function (car args) value))
         ;; `rplaca` returns the cons and `setf` must return the value, so the
         ;; value is bound once and handed back. A gensym, because the place
         ;; and the value are the caller's code and may use any name they like
         ;; (chapter 15).
         ((eq head 'car) (%setf-through 'rplaca (car args) value))
         ((eq head 'cdr) (%setf-through 'rplacd (car args) value))
         ((eq head 'first) (%setf-through 'rplaca (car args) value))
         ((eq head 'rest) (%setf-through 'rplacd (car args) value))
         ;; Anything else: ask the table. `defstruct` registers its
         ;; accessors there, which is how `(setf (point-x p) 1)` works on a
         ;; structure the prelude has never heard of.
         ;;
         ;; Common Lisp does this with `define-setf-expander`, which hands
         ;; the *macro* five values describing how to read and write the
         ;; place, so a place can evaluate its subforms exactly once even
         ;; when something like `incf` reads and writes it. This table maps a
         ;; reader's name to a writer's name and nothing more. It is enough
         ;; for `defstruct`; for `incf`, the library's `get-setf-expansion`
         ;; makes the five values out of this same entry.
         (t (let ((setter (gethash head %setf-setters)))
              (if setter
                  (cons setter (append args (list value)))
                  nil))))))
    (t nil)))

(defun %setf-unknown-place (place)
  (error "SETF does not know how to set this place" place))

;; The cons is bound before the value, because Common Lisp evaluates a
;; place's subforms before the new value (CLHS 5.1.1.1). This bound the value
;; alone until #52, so `(setf (car (f)) (g))` called `g` first; SBCL calls `f`.
(defun %setf-through (setter cons-form value)
  (let ((cell (gensym))
        (once (gensym)))
    (list 'let (list (list cell cons-form) (list once value))
          (list setter cell once)
          once)))

;; `(setf place value place value ...)`, left to right, yielding the last
;; value — which is what CLHS says, and why an odd number of forms is an
;; error rather than a place with nothing to put in it.
(defmacro setf (&rest pairs)
  (labels ((expand (rest)
             (cond ((null rest) nil)
                   ((null (cdr rest))
                    (error "SETF wants place and value in pairs" (car rest)))
                   (t (cons (%setf-place (car rest) (car (cdr rest)))
                            (expand (cdr (cdr rest))))))))
    (let ((forms (expand pairs)))
      (cond ((null forms) nil)
            ((null (cdr forms)) (car forms))
            (t (cons 'progn forms))))))

;;; ------------------------------------------------------------------
;;; Conditions
;;; ------------------------------------------------------------------

;; `handler-case` is a **macro** in Common Lisp, over `handler-bind` and
;; `block`. Quokka has no `handler-bind`, so the machinery is the special
;; operator `%handler-case` — but the name people write stays a macro, which
;; keeps the special operator count honest and leaves somewhere to check
;; clauses before the evaluator sees them.
;;
;; A clause is `(type (var) body...)`, or `(type () body...)` to ignore the
;; condition. Clauses are tried in order and the first matching type wins.
;;
;; **Type matching is flat.** `error`, `condition` and `t` catch everything;
;; anything else must name the condition's own type exactly. Common Lisp
;; matches by subtype through the class system, which is phase 3. Written
;; down in `docs/design.md` rather than discovered.
(defmacro handler-case (form &rest clauses)
  (cons '%handler-case (cons form clauses)))

;; Run a form; if it signals, yield nil and the condition — two values, so a
;; caller can tell "it failed" from "it returned nil".
(defmacro ignore-errors (&rest forms)
  `(handler-case (progn ,@forms) (error (%ignored-condition) (values nil %ignored-condition))))

;; Called from the code `lambda_list::destructure` generates, when a macro
;; call does not fit the shape its parameter list described. These were
;; missing entirely until there was something to signal into.
(defun %destructuring-too-short ()
  (error "a macro call is missing arguments its parameter list asked for"))

(defun %destructuring-too-long ()
  (error "a macro call has more arguments than its parameter list asked for"))

;; `&key` in a pattern (#76). The rest of the list is a property list, and
;; there is no argument binder to search it, so the generated code calls
;; these. The first checks it once: an even length, and no keyword the
;; pattern does not name, unless other keys are allowed by `&allow-other-keys`
;; or by a true `:allow-other-keys` among the arguments (CLHS 3.4.1.4.1).
(defun %destructuring-keys (plist keywords allow)
  (%destructuring-check-keys
   plist keywords
   (if allow t (%destructuring-key plist :allow-other-keys nil))))

(defun %destructuring-check-keys (plist keywords allow)
  (cond ((null plist) nil)
        ((not (consp (cdr plist))) (%destructuring-too-long))
        ((or allow (eq (car plist) :allow-other-keys) (%destructuring-known (car plist) keywords))
         (%destructuring-check-keys (cdr (cdr plist)) keywords allow))
        (t (%destructuring-too-long))))

(defun %destructuring-known (keyword keywords)
  (cond ((null keywords) nil)
        ((eq keyword (car keywords)) t)
        (t (%destructuring-known keyword (cdr keywords)))))

;; The value after KEYWORD, the leftmost if it appears twice, or DEFAULT.
(defun %destructuring-key (plist keyword default)
  (cond ((not (consp plist)) default)
        ((eq (car plist) keyword) (car (cdr plist)))
        (t (%destructuring-key (cdr (cdr plist)) keyword default))))

;;; ------------------------------------------------------------------
;;; defstruct
;;; ------------------------------------------------------------------

;; `(defstruct point x y)` defines a record type: a constructor, a predicate,
;; an accessor per slot, each settable with `setf`, and a copier.
;;
;; This is the macro that justifies the rest. It writes **nine or ten
;; definitions** from one line, each of them a `defun` the user could have
;; typed, and it needs almost everything the last few chapters built: `&key`
;; lambda lists for the constructor, vectors to store the slots in, `gensym`
;; so its generated code cannot capture a caller's names, a hash table so
;; `setf` can find the writers, and `error` to complain about a malformed
;; slot. A compiler will represent nearly everything it works with this way.
;;
;; **The representation is a vector with the type name in slot 0.** Common
;; Lisp's `defstruct` defines a genuine type — `typep` knows it, the printer
;; shows `#S(POINT :X 1 :Y 2)`, and `:include` gives it single inheritance.
;; Quokka has no class system until phase 3, so a structure here *is* a
;; vector and prints as one. `point-p` is therefore slightly generous: a
;; vector someone else built with the symbol `point` in slot 0 would pass it.
;;
;; Not supported: the option list `(defstruct (point (:constructor ...)) ...)`,
;; `:include`, `:print-function`, and slot types. The simple form is what a
;; tier 0 compiler needs.

(defun %slot-name (slot)
  (cond ((symbolp slot) slot)
        ((consp slot) (car slot))
        (t (error "a defstruct slot wants a name or (name default)" slot))))

(defun %slot-default (slot)
  (if (consp slot) (car (cdr slot)) nil))

;; `point` and `x` become the symbol `POINT-X`. Symbols are interned by name,
;; so building a name out of two others is building a string and interning
;; it — which is `intern`, and which Quokka does not expose yet, so the
;; primitive does the work.
(defun %symbol-concat (a b)
  (%intern (%string-append (symbol-name a) (symbol-name b))))

(defmacro defstruct (name &rest slots)
  (if (not (symbolp name))
      (error "defstruct wants a symbol for a name" name))
  (let ((names (%map-slot-names slots)))
    (cons 'progn
          (append
           (list (%defstruct-constructor name slots))
           (list (%defstruct-predicate name))
           (list (%defstruct-copier name))
           (%defstruct-accessors name names 1)
           (list (list 'quote name))))))

(defun %map-slot-names (slots)
  (labels ((walk (rest acc)
             (if (endp rest)
                 (reverse acc)
                 (walk (cdr rest) (cons (%slot-name (car rest)) acc)))))
    (walk slots nil)))

(defun %defstruct-constructor (name slots)
  ;; (defun make-point (&key (x nil) (y nil)) (vector 'point x y))
  (let ((parameters
          (labels ((walk (rest acc)
                     (if (endp rest)
                         (reverse acc)
                         (walk (cdr rest)
                               (cons (list (%slot-name (car rest))
                                           (%slot-default (car rest)))
                                     acc)))))
            (walk slots nil))))
    (list 'defun
          (%symbol-concat 'make- name)
          (cons '&key parameters)
          (cons 'vector (cons (list 'quote name) (%map-slot-names slots))))))

(defun %defstruct-predicate (name)
  ;; A vector whose first element is this type's name. Generous, as the
  ;; comment above says, and exact enough until there are real types.
  (let ((x (gensym)))
    (list 'defun (%symbol-concat name '-p) (list x)
          (list 'and
                (list 'vectorp x)
                (list '> (list 'length x) 0)
                (list 'eq (list 'aref x 0) (list 'quote name))))))

(defun %defstruct-copier (name)
  (let ((x (gensym)))
    (list 'defun (%symbol-concat 'copy- name) (list x)
          (list 'apply (list 'function 'vector) (list '%vector-to-list x)))))

;; One accessor and one writer per slot, and the writer registered so `setf`
;; can find it. The index is computed here, at macroexpansion time, and
;; appears in the generated code as a literal.
(defun %defstruct-accessors (name names index)
  (if (endp names)
      nil
      (let ((reader (%symbol-concat name (%symbol-concat '- (car names))))
            (x (gensym))
            (v (gensym)))
        (let ((writer (%symbol-concat '%set- reader)))
          (cons (list 'defun reader (list x) (list 'aref x index))
                (cons (list 'defun writer (list x v)
                            (list 'setf (list 'aref x index) v))
                      (cons (list '%define-setf-writer
                                  (list 'quote reader)
                                  (list 'quote writer))
                            (%defstruct-accessors name (cdr names) (+ index 1)))))))))

;;; ------------------------------------------------------------------
;;; Multiple values
;;; ------------------------------------------------------------------

;; `multiple-value-call` and `multiple-value-prog1` are the two operators;
;; everything anyone actually writes is a macro over them.
;;
;; `&optional` gives the missing-values rule for free — a variable with no
;; value gets nil — and `&rest` discards the extras. The lambda list work from
;; chapter 18 doing a job nobody had it in mind for.
(defmacro multiple-value-bind (variables form &body body)
  (let ((extra (gensym)))
    `(multiple-value-call
         (lambda (&optional ,@variables &rest ,extra)
           ,extra
           ,@body)
       ,form)))

(defmacro multiple-value-list (form)
  `(multiple-value-call (function list) ,form))

(defmacro nth-value (n form)
  `(nth ,n (multiple-value-list ,form)))

