;;;; Top-level forms (#104, CLHS 3.2.3.1): the parts of a top-level `progn`
;;;; are top-level forms themselves, each evaluated before the next is looked
;;;; at — so a definition made by one is seen by the ones after it. The same
;;;; holds through `locally`, `macrolet`, `symbol-macrolet` and `eval-when`.

;;; A macro defined and used in one progn.
(progn (defmacro tl-one () 1) (tl-one))

;;; A structure, and a setf of its accessor, in one progn.
(progn (defstruct tl-point x y)
       (let ((p (make-tl-point :x 1 :y 2)))
         (setf (tl-point-x p) 10)
         (list (tl-point-x p) (tl-point-y p))))

;;; defsetf, then its use.
(progn (defun tl-first (x) (car x))
       (defsetf tl-first rplaca)
       (let ((x (list 1 2))) (setf (tl-first x) 9) x))

;;; A global symbol macro.
(progn (define-symbol-macro tl-answer 42) tl-answer)

;;; Nested progns are taken apart all the way down.
(progn (progn (defmacro tl-two () 2)) (progn (progn (tl-two))))

;;; The values are the last form's, all of them; an empty progn is nil.
(multiple-value-list (progn 1 (values 2 3)))
(progn)

;;; The same through the forms that bind nothing at run time.
(locally (defmacro tl-three () 3) (tl-three))
(macrolet ((local () 7))
  (defmacro tl-seven () '(local))
  (list (local) (tl-seven)))
(symbol-macrolet ((s 5))
  (defmacro tl-five () 5)
  (list s (tl-five)))
(macrolet ((local () 8)) (progn (defmacro tl-eight () 8) (list (local) (tl-eight))))

;;; eval-when: :execute is what `eval` and `load` of source see.
(eval-when (:execute) (defmacro tl-when () ''ran) (tl-when))
(eval-when (:compile-toplevel) (print 'never))
(eval-when (:compile-toplevel :load-toplevel :execute) 6)

;;; A defining macro that expands into a progn — why this matters at all.
(defmacro tl-define (name value)
  `(progn (defmacro ,name () ,value) (,name)))
(tl-define tl-made 99)

;;; `eval` called as a function does the same.
(eval '(progn (defmacro tl-evald () ''yes) (tl-evald)))

;;; A progn that is not at top level is compiled whole, as it always was:
;;; here the macro does not exist when the `let` is compiled, so the call is
;;; an ordinary call to a function that does not exist.
(handler-case (let () (progn (defmacro tl-late () 1) (tl-late)))
  (error () 'not-yet-a-macro))
