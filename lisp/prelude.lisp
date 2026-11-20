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
;; **These are the list cases only.** In Common Lisp `length` and `reverse`
;; work on any sequence, and this prelude has no vectors or strings to work
;; on yet. They are correct for what exists and will be replaced, not
;; extended, when sequences arrive.

(defun length (list)
  (labels ((walk (rest n)
             (if (endp rest) n (walk (cdr rest) (+ n 1)))))
    (walk list 0)))

(defun reverse (list)
  (labels ((walk (rest acc)
             (if (endp rest) acc (walk (cdr rest) (cons (car rest) acc)))))
    (walk list nil)))

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
