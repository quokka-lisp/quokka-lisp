;;;; The last three special operators — progv, load-time-value,
;;;; symbol-macrolet — and the lexical environment macros see (#53).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.

(defvar *x* 1)
(defvar *y* 2)
(defun current () (list *x* *y*))

;; progv binds names computed at run time, dynamically: a function called
;; inside sees them, and they end with the progv.
(progv '(*x*) '(10) (current))
(current)
(progv (list '*x* '*y*) (list 'a 'b) (current))
(let ((names (list '*y*))) (progv names '(20) (current)))
(progv '(*x*) '(5 6 7) *x*)
(progv nil nil 'empty)

;; Fewer values than names: the rest are bound and unbound.
(progv '(*x* *y*) '(30) (list *x* (boundp '*y*)))
(list (boundp '*y*) *y*)

;; A name nobody proclaimed is still bound dynamically.
(progv '(free-name) '(40) (symbol-value 'free-name))
(boundp 'free-name)

;; Every way out restores the old values.
(catch 'out (progv '(*x*) '(50) (throw 'out *x*)))
*x*
(block b (progv '(*x*) '(51) (return-from b *x*)))
*x*
(handler-case (progv '(*x*) '(52) (error "inside")) (error () *x*))
(progv '(*x*) '(53) (progv '(*x*) '(54) *x*))
*x*

;; The body's values all come back, and the lexical variables around it are
;; visible inside.
(multiple-value-list (progv '(*x*) '(60) (values *x* 'second)))
(let ((local 'lexical)) (progv '(*x*) '(61) (list local *x*)))

;; load-time-value: evaluated once, when the code is compiled, and then a
;; constant of it.
(defvar *made* 0)
(defun made-once () (load-time-value (setq *made* (+ *made* 1))))
*made*
(list (made-once) (made-once) *made*)
(defun shared-cell () (load-time-value (list 'cell)))
(eq (shared-cell) (shared-cell))
(progn (setf (car (shared-cell)) 'changed) (shared-cell))
(load-time-value (+ 1 2) t)

;; symbol-macrolet: a symbol that stands for a form, as a value and a place.
(let ((cell (list 1 2)))
  (symbol-macrolet ((head (car cell)))
    (list head (setq head 5) (incf head) (push 'x (cdr cell)) cell)))
(symbol-macrolet ((x 1)) (let ((x 2)) x))
(let ((x 'outer)) (symbol-macrolet ((x 'inner)) x))
(symbol-macrolet ((x 'outer)) (symbol-macrolet ((x 'inner)) x))
(let ((a 1) (b 2)) (symbol-macrolet ((x a) (y b)) (setq x 10 y 20) (list a b)))
(let ((v (vector 1 2)))
  (symbol-macrolet ((first-of (aref v 0)) (second-of (aref v 1)))
    (rotatef first-of second-of)
    v))

;; Inside a closure, and past other scopes: the symbol macro takes no room
;; at run time, so the variables around it are still found.
(let ((a 1))
  (symbol-macrolet ((x a))
    (let ((b 2))
      (funcall (lambda (c) (list x b c)) 3))))
(let ((n 10))
  (symbol-macrolet ((counter n))
    (flet ((bump () (incf counter)))
      (bump) (bump) n)))

;; incf on a symbol macro evaluates what it stands for once: this is why
;; macros need the environment.
(let ((calls 0) (cells (list (list 100))))
  (symbol-macrolet ((x (car (progn (setq calls (+ calls 1)) (car cells)))))
    (incf x 5)
    (list calls cells)))

;; A macro that asks for its environment sees symbol macros and local macros.
(defmacro expansion-here (form &environment env)
  (list 'quote (multiple-value-list (macroexpand-1 form env))))
(symbol-macrolet ((x (car cell))) (expansion-here x))
(macrolet ((answer () 42)) (expansion-here (answer)))
(expansion-here plain)
(multiple-value-list (macroexpand-1 'plain))
(multiple-value-list (macroexpand '(answer)))

;; A local macro is a place, through its expansion.
(let ((v (vector 1 2)))
  (macrolet ((second-of (x) (list 'aref x 1)))
    (incf (second-of v) 10)
    (setf (second-of v) (+ (second-of v) 1))
    v))
