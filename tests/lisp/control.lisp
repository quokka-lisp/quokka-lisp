;;; Special operators that decide whether their arguments are evaluated.

;; quote evaluates nothing at all, which is its entire purpose.
(quote foo)
'foo
''foo
'(undefined-function 1)

;; if evaluates exactly one branch. The untaken branch here would signal if it
;; were evaluated, which is the point of writing it that way.
(if t 1 2)
(if nil 1 2)
(if t 1 (undefined-function))
(if nil (undefined-function) 2)

;; Everything but nil is true, including zero and the empty string.
(if 0 'yes 'no)
(if "" 'yes 'no)
(if '() 'yes 'no)
(if 'something 'yes 'no)

;; A missing else branch yields nil.
(if nil 1)
(if t 1)

(progn)
(progn 1)
(progn 1 2 3)

(setq a 1)
a
(setq a 1 b 2)
(+ a b)
(setq a (+ a 10))
a
