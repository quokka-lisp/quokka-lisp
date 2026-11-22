;;;; Special variables: defvar, defparameter, and dynamic binding (#40).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted: the interpreter stays tier 0 (#39). Its baseline came from
;;;; SBCL through scripts/oracle.lisp, the reference itself.

(defvar *depth* 0)
(defun show-depth () *depth*)

;; A binding is visible to everything called inside it, and ends with it.
(show-depth)
(let ((*depth* 5)) (show-depth))
(show-depth)

;; defvar leaves a bound variable alone; defparameter always assigns.
(defvar *depth* 99)
*depth*
(defparameter *level* 1)
(defparameter *level* 2)
*level*

;; Assignment inside a binding changes the binding, not the global.
(defun bump () (setq *depth* (+ *depth* 1)))
(let ((*depth* 100)) (bump) (bump) (show-depth))
*depth*

;; Every way out restores the old value.
(catch 'out (let ((*depth* 7)) (throw 'out (show-depth))))
*depth*
(block b (let ((*depth* 8)) (return-from b (show-depth))))
*depth*
(handler-case (let ((*depth* 9)) (error "boom")) (error () (show-depth)))
*depth*
(let ((n 0))
  (tagbody top
     (let ((*depth* n))
       (setq n (+ n 1))
       (if (< n 3) (go top))))
  (list n *depth*))

;; A cleanup runs inside the binding it protects.
(let ((*depth* 6)) (unwind-protect (show-depth) (bump)))
*depth*

;; Nested bindings unwind innermost first.
(let ((*depth* 1)) (let ((*depth* 2)) (show-depth)))
(let ((*depth* 3)) (list (let ((*depth* 4)) (show-depth)) (show-depth)))

;; A parameter that names a special variable binds it dynamically.
(defun with-depth (*depth*) (show-depth))
(with-depth 42)
*depth*

;; let evaluates every init before binding anything; let* one at a time.
(let ((*depth* 50) (x *depth*)) x)
(let* ((*depth* 50) (x *depth*)) x)
(let ((a 1) (*depth* 2)) (list a (show-depth)))

;; A closure captures lexical variables, never a dynamic binding.
(defvar *who* 'outer)
(defun who () *who*)
(let ((f (let ((*who* 'inner)) (lambda () (who))))) (funcall f))

;; boundp, symbol-value and set reach the value cell by name.
(defvar *unset*)
(boundp '*unset*)
(let ((*unset* 5)) (boundp '*unset*))
(boundp '*unset*)
(symbol-value '*depth*)
(set '*depth* 11)
*depth*
(setq *depth* 0)
