;;; The prelude: defun, when, unless, cond, and, or, and the list functions.
;;;
;;; None of this is implemented in Rust. Every form below is a macro or a
;;; function defined in `lisp/prelude.lisp`, which makes this file the real
;;; test of rule 1: if Quokka and SBCL agree here, the things that moved above
;;; the primitive table still mean what Common Lisp says they mean.

;; defun returns the name, and defines in the function cell (Lisp-2).
(defun square (x) (* x x))
(square 9)
(defun ignore-me (x) x)

;; The implicit block. CLHS: the body of a defun is wrapped in a block named
;; after the function, so return-from works without writing the block.
(defun first-zero (list)
  (if (null list)
      'none
      (if (= (car list) 0)
          (return-from first-zero 'found)
          (first-zero (cdr list)))))
(first-zero '(3 2 1 0))
(first-zero '(3 2 1))

;; A block the body does not use changes nothing.
(defun plain (x) (+ x 1))
(plain 1)

;; when and unless.
(when t 1 2 3)
(when nil 1)
(unless nil 'yes)
(unless t 'no)
(when (= 1 1) 'equal)

;; The body of a false when is not evaluated at all.
(let ((seen nil))
  (when nil (setq seen 'ran))
  seen)
(let ((seen nil))
  (unless t (setq seen 'ran))
  seen)

;; cond, including the empty case and a clause with no body.
(cond)
(cond (nil 1) (t 2))
(cond (nil 1))
(cond ((= 1 1) 'first) ((= 1 1) 'second))
(cond ((+ 1 2)))
(cond (nil 1) ((car '(a)) 'from-car))

;; A cond clause with no body yields the test's value, evaluated once.
(let ((n 0))
  (cond ((setq n (+ n 1))))
  n)

;; and and or: the value, not just a boolean.
(and)
(and 1)
(and 1 2 3)
(and 1 nil 3)
(and nil 'never)
(or)
(or 1)
(or nil nil 3)
(or 'first 'second)
(or nil nil)

;; Short-circuiting is the point: the rest is not evaluated.
(let ((n 0))
  (and nil (setq n 1))
  n)
(let ((n 0))
  (or t (setq n 1))
  n)

;; or must not capture a caller's variable, whatever it is called.
(let ((once 'outer)) (or nil once))
(let ((value 'outer)) (or nil value))
(let ((tmp 'outer)) (or nil tmp))

;; Nested, because macros expanding into macros is the normal case.
(cond ((and t t) (or nil 'deep)))
(when (or nil t) (unless nil 'nested))

;; The list functions, which are Lisp rather than primitives.
(car '(1 2 3))
(cadr '(1 2 3))
(caddr '(1 2 3))
(cddr '(1 2 3))
(caar '((1 2) 3))
(cdar '((1 2) 3))
(first '(1 2 3))
(second '(1 2 3))
(third '(1 2 3))
(rest '(1 2 3))
(not nil)
(not t)
(not '(1))
(endp nil)
(endp '(1))
(length nil)
(length '(1 2 3))
(reverse nil)
(reverse '(1 2 3))
(nth 0 '(a b c))
(nth 2 '(a b c))
(nth 9 '(a b c))
(member 'b '(a b c))
(member 'z '(a b c))
(member 2 '(1 2 3))

;; Tail recursion through a defun, which the implicit block must not defeat.
(defun count-down (n) (if (= n 0) 'done (count-down (- n 1))))
(count-down 10000)

;; A defun that both recurses in tail position and returns from its block,
;; which is the case the expansion-time scan has to get right.
(defun find-key (key alist)
  (cond ((null alist) nil)
        ((eq (caar alist) key) (return-from find-key (cdar alist)))
        (t (find-key key (cdr alist)))))
(find-key 'b '((a . 1) (b . 2) (c . 3)))
(find-key 'z '((a . 1) (b . 2)))
(find-key 'c '((a . 1) (b . 2) (c . 3)))
