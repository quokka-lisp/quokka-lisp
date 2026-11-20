;;; lambda, defun, closures and scope.
;;;
;;; Nothing here prints a function: SBCL renders one with an address, so no
;;; output would ever match.

((lambda (x) (* x x)) 7)
((lambda () 42))
((lambda (a b) (- a b)) 10 3)

(defun square (x) (* x x))
(square 9)
(square (square 2))

;; A body is an implicit progn, and yields its last form.
(defun several (x) (+ x 1) (+ x 2))
(several 10)

;; Scope: inner bindings shadow outer ones, and leave them alone.
(setq x 'global)
((lambda (x) x) 'local)
x
((lambda (y) x) 'local)
((lambda (x) ((lambda (x) x) 'inner)) 'outer)
((lambda (x) ((lambda (y) x) 'inner)) 'outer)

;; A closure keeps the scope it was made in, after the call that made it has
;; returned.
(defun adder (n) (lambda (x) (+ x n)))
(funcall (adder 5) 37)
;; Wrapped so the closure is not the value of a top-level form: SBCL prints a
;; function with its address, and no output would match. The README says so,
;; and this file broke the rule on its first run — which is why
;; check-differential.sh now enforces it instead of asking politely.
(progn (setq add5 (adder 5)) 'bound)
(funcall add5 1)
(funcall add5 95)

;; Common Lisp is a Lisp-2: the same name can be a function and a variable.
(defun thing (x) (* x 2))
(setq thing 5)
(thing thing)
thing

;; Recursion. 20! is the largest factorial fitting in a fixnum, so this is the
;; most that can be asked without needing bignums.
(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
(fact 0)
(fact 1)
(fact 5)
(fact 20)

;; Tail recursion, which must run in constant space.
(defun down (n) (if (= n 0) 'done (down (- n 1))))
(down 10000)

;; Wrong argument counts signal.
(square 1 2)
(square)
