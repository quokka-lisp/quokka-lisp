;;; Multiple values, and macrolet.
;;;
;;; A Common Lisp form may return any number of values, and almost nothing
;;; looks at more than the first — which is the part that is easy to get
;;; wrong. `(+ (values 1 2) 3)` is 4, not an error and not 6, because extra
;;; values are *transient*: anything that is not a `multiple-value-` form
;;; takes the primary and the rest are gone.
;;;
;;; Every form below is wrapped in something that fixes how many values are
;;; being looked at, because the test harness prints one value per form and
;;; "how many values came back" is exactly what is under test.

;; Extra values are dropped by ordinary forms.
(+ (values 1 2) 3)
(list (values 1 2))
(car (list (values 1 2 3)))
(let ((x (values 1 2))) x)
(if (values t nil) 'yes 'no)

;; No values at all behaves as nil where one is wanted.
(list (values))
(+ 1 (values))

;; multiple-value-list is the direct way to see how many there were.
(multiple-value-list (values 1 2 3))
(multiple-value-list (values))
(multiple-value-list (values 1))
(multiple-value-list 7)
(multiple-value-list (values 'a "b" 3))
(multiple-value-list (values-list '(1 2 3)))
(multiple-value-list (values-list nil))

;; multiple-value-call: every value of every form, in order.
(multiple-value-call #'list (values 1 2) (values 3 4))
(multiple-value-call #'list 1 2)
(multiple-value-call #'+ (values 1 2) (values 3 4))
(multiple-value-call #'list (values))
(multiple-value-call #'list)
(multiple-value-call #'list (values 1) (values) (values 2 3))
(multiple-value-call #'list 'a (values 'b 'c) 'd)

;; multiple-value-prog1 keeps the first form's values and runs the rest.
(multiple-value-list (multiple-value-prog1 (values 1 2) 'ignored))
(multiple-value-list (multiple-value-prog1 (values 1 2) (values 3 4)))
(multiple-value-list (multiple-value-prog1 7))
(let ((n 0))
  (multiple-value-prog1 (values 'a 'b) (setq n 1) (setq n (+ n 1)))
  n)
(let ((n 0))
  (list (multiple-value-list (multiple-value-prog1 (values 'a 'b) (setq n 9)))
        n))

;; multiple-value-bind: missing variables are nil, extra values are dropped.
(multiple-value-bind (a b) (values 1 2) (list a b))
(multiple-value-bind (a b c) (values 1 2) (list a b c))
(multiple-value-bind (a) (values 1 2 3) a)
(multiple-value-bind (a b) 7 (list a b))
(multiple-value-bind () (values 1 2) 'body-ran)
(multiple-value-bind (a b) (values 1 2) (setq a (+ a b)) a)

;; The bound variables are ordinary lexical variables.
(multiple-value-bind (a b) (values 1 2)
  (multiple-value-bind (c d) (values a b)
    (list a b c d)))

;; nth-value.
(nth-value 0 (values 'a 'b 'c))
(nth-value 1 (values 'a 'b 'c))
(nth-value 2 (values 'a 'b 'c))
(nth-value 9 (values 'a 'b 'c))

;; Values flow out of a function call unchanged.
(defun two-of-them () (values 'first 'second))
(multiple-value-list (two-of-them))
(multiple-value-bind (a b) (two-of-them) (list a b))
(+ 1 (if (two-of-them) 1 2))

;; ...and through every form that is transparent to values, which is most of
;; them. Each of these was `(1)` at some point during the implementation.
(multiple-value-list (block b (values 1 2)))
(multiple-value-list (block b (return-from b (values 1 2))))
(multiple-value-list (let ((x 1)) (values x 2)))
(multiple-value-list (let* ((x 1)) (values x 2)))
(multiple-value-list (progn 'ignored (values 1 2)))
(multiple-value-list (if t (values 1 2) 'no))
(multiple-value-list (catch 'tag (values 1 2)))
(multiple-value-list (catch 'tag (throw 'tag (values 1 2))))
(multiple-value-list (handler-case (values 1 2) (error () 'no)))
(multiple-value-list (handler-case (error "x") (error () (values 1 2))))
(multiple-value-list (flet ((f () (values 1 2))) (f)))
(multiple-value-list (locally (values 1 2)))

;; unwind-protect yields the protected form's values, and a cleanup running
;; in between must not cost the second one.
(multiple-value-list (unwind-protect (values 1 2) 'cleanup))
(let ((n 0))
  (list (multiple-value-list (unwind-protect (values 1 2) (setq n 5))) n))
(multiple-value-list (catch 'tag (unwind-protect (throw 'tag (values 1 2)) 'c)))
(multiple-value-list (block b (unwind-protect (return-from b (values 1 2)) 'c)))

;; gethash returns two values, which is the only way to tell a stored nil
;; from a missing key.
(let ((h (make-hash-table)))
  (setf (gethash 'k h) nil)
  (multiple-value-list (gethash 'k h)))
(let ((h (make-hash-table)))
  (multiple-value-list (gethash 'missing h)))
(let ((h (make-hash-table)))
  (setf (gethash 'k h) 'stored)
  (multiple-value-list (gethash 'k h)))

;; macrolet: a macro with a scope.
(macrolet ((twice (x) (list '+ x x)))
  (twice 5))
(macrolet ((twice (x) (list '+ x x)))
  (list (twice 1) (twice 2)))
(macrolet ((one () 1) (two () 2))
  (list (one) (two)))

;; The binding is lexical, so it is gone outside the macrolet.
(defun twice (x) (list 'called-the-function x))
(list (macrolet ((twice (x) (list '+ x x))) (twice 3))
      (twice 3))

;; A macrolet shadows a global macro of the same name, inside its body only.
(defmacro shadowed () ''global)
(list (macrolet ((shadowed () ''local)) (shadowed))
      (shadowed))

;; macrolet bodies are ordinary macro bodies: they can destructure.
(macrolet ((pick ((a b)) (list 'quote a)))
  (pick (x y)))
