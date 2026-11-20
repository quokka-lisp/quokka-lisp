;;; Cons cells and the operations that define them.

(cons 1 2)
(cons 1 nil)
(cons 1 (cons 2 nil))
(cons nil nil)

(quote (1 2 3))
'(1 2 3)
'()
'(1 . 2)
'(1 2 . 3)
'(a (b c) d)

(car '(1 2 3))
(cdr '(1 2 3))
(car (cdr '(1 2 3)))
(cdr (cdr '(1 2 3)))

;; car and cdr of nil are nil in Common Lisp, which a great deal of list code
;; relies on. An implementation that signalled here would be wrong.
(car nil)
(cdr nil)

(null nil)
(null '())
(null '(1))
(null 0)

(consp '(1))
(consp nil)
(consp 1)

(symbolp 'x)
(symbolp 1)

;; Nested structure survives printing and reading back.
'((1 2) (3 4))
'(1 "two" #\3)
