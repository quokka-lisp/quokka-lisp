;;; Fixnum arithmetic.
;;;
;;; Nothing here may overflow a fixnum: SBCL would promote to a bignum and
;;; Quokka would signal, and that difference is real rather than a bug.

(+ 1 2)
(+ 1 2 3 4 5)
(+)
(+ 7)

(* 6 7)
(*)
(* 2 3 4)

(- 10 3)
(- 5)
(- 100 1 2 3)

(= 1 1)
(= 1 2)
(= 1 1 1)

(< 1 2)
(< 1 2 3)
(< 1 3 2)
(> 3 2 1)
(> 1 2)

;; Negative numbers and zero, which is where off-by-one errors live.
(+ -1 1)
(* -1 -1)
(- 0 1)
(< -1 0)

;; Wrong types and wrong counts must signal in both.
(+ 1 'a)
(- )
(car 1)
