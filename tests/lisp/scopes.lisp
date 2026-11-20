;;; let, let*, flet, labels, function and the quiet ones.
;;;
;;; Most of this is behaviour I guessed at while implementing, which is exactly
;;; what the oracle is for. `(let ((x 1) (x 2)) x)` is deliberately absent: the
;;; standard leaves duplicate bindings undefined, so a test asserting anything
;;; would be testing an accident.

(let () 1)
(let ((x 1)) x)
(let ((x 1) (y 2)) (+ x y))
(let ((x 1)) (let ((y 2)) (+ x y)))

;; A binding with no init, and a bare name, both bind nil.
(let ((x)) x)
(let (x) x)
(let (x y) (list x y))

;; The difference between let and let*: every init of a let is evaluated in the
;; OUTER scope, so the inner x is not visible to y.
(let ((x 1)) (let ((x 2) (y x)) y))
(let ((x 1)) (let* ((x 2) (y x)) y))

(let* () 1)
(let* ((x 1) (y (+ x 1))) (* x y))
(let* ((x 1) (x (+ x 1))) x)

;; A body is an implicit progn and yields its last form.
(let ((x 1)) x (+ x 1))
(let ((x 1)))

;; Shadowing, and that it is undone on the way out.
(setq g 'global)
(let ((g 'inner)) g)
g

;; setq inside a let touches the binding, not the global.
(setq h 'outer)
(let ((h 'inner)) (setq h 'changed) h)
h

(flet ((f (x) (* x 2))) (f 21))
(flet ((f (x) (* x 2)) (g (x) (+ x 1))) (f (g 20)))

;; A Lisp-2 keeps the namespaces apart: the same name can be a local variable
;; and a local function at once, and neither disturbs the other.
;;
;; The name must not be a COMMON-LISP symbol. Binding one as a local function
;; is undefined behaviour (CLHS 11.1.2.1.2) and SBCL enforces it with a package
;; lock — which it used to catch an earlier version of this file that shadowed
;; CAR. Quokka has no packages and allowed it, so the two disagreed for a
;; reason that was the test's fault.
(let ((f 5)) (flet ((f (x) (* x 2))) (list f (f 10))))
(flet ((f (x) (* x 2))) (let ((f 5)) (list f (f 10))))
(let ((g 1)) (flet ((g () 2)) (+ g (g))))

;; flet bindings cannot see each other; labels bindings can.
(labels ((even-p (n) (if (= n 0) t (odd-p (- n 1))))
         (odd-p (n) (if (= n 0) nil (even-p (- n 1)))))
  (list (even-p 10) (odd-p 10)))
(labels ((down (n) (if (= n 0) 'done (down (- n 1))))) (down 1000))

(funcall #'car '(1 2))
(funcall #'+ 1 2 3)
(funcall (function cdr) '(1 2))
(funcall #'(lambda (x) (* x 3)) 14)

(the integer 42)
(the t (+ 1 1))
(locally 1 2)
(locally)

(eval-when (:execute) 'ran)
(eval-when (:compile-toplevel) 'not-run)
(eval-when () 'not-run)

;; Malformed forms signal.
(let)
(let* 1)
(flet ((f)) 1)
(function)
(function nosuchfunction)
