;;;; Integers of any size (#16, step L).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;; Unlike every other file here, this one is about bignums.

(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
(defun two-to (n) (expt 2 n))

;; Crossing the fixnum boundary, both ways.
(list most-positive-fixnum most-negative-fixnum)
(list (+ most-positive-fixnum 1) (- most-negative-fixnum 1) (- most-negative-fixnum))
(list (- (+ most-positive-fixnum 1) 1) (+ (- most-negative-fixnum 1) 1))
(list (integerp (+ most-positive-fixnum 1)) (numberp (fact 30)))
(* 2432902008176640000 21)
(list (fact 25) (fact 30) (fact 50))
(* most-negative-fixnum -1)
(* most-negative-fixnum most-negative-fixnum)

;; Addition and subtraction, mixed fixnums and bignums, every sign.
(list (+ (fact 30) 1) (+ 1 (fact 30)) (+ (fact 30) (- (fact 30))) (- (fact 30) (fact 29)))
(list (+ (- (fact 30)) (fact 29)) (- (fact 29) (fact 30)) (- (- (fact 30))))
(+ (two-to 100) (two-to 100) 1 2 3)
(- (two-to 90) 1 1 1)

;; Multiplication.
(* 123456789012345678901234567890 -987654321098765432109876543210)
(* (fact 20) (fact 20) (fact 20))
(* (two-to 200) 0)
(list (expt 3 100) (expt -2 63) (expt 10 40) (expt 7 0) (expt 0 0))

;; Comparison.
(list (= (fact 25) (* 25 (fact 24))) (= (fact 25) (fact 24)) (/= (fact 25) (fact 24)))
(list (< (fact 25) (fact 26)) (< (- (fact 26)) (- (fact 25))) (> (fact 30) most-positive-fixnum)
      (< most-negative-fixnum (- (fact 30)) 0))
(list (<= 1 (fact 30) (fact 30)) (>= (fact 30) (fact 29) 5) (max 1 (fact 22) 3) (min (- (fact 22)) 7))
(list (zerop (- (fact 30) (fact 30))) (plusp (fact 30)) (minusp (- (fact 30))) (abs (- (fact 30))))
(list (evenp (fact 30)) (oddp (+ (fact 30) 1)) (signum (- (fact 30))) (signum (fact 30)) (signum 0))

;; Division: truncate, floor, ceiling, round, mod, rem, by fixnums and by
;; bignums of several limbs, with every combination of signs.
(multiple-value-list (truncate (fact 30) (fact 20)))
(multiple-value-list (truncate (expt 10 50) (+ (expt 10 25) 7)))
(multiple-value-list (truncate (- (expt 10 50)) (+ (expt 10 25) 7)))
(multiple-value-list (truncate (expt 10 50) (- (+ (expt 10 25) 7))))
(multiple-value-list (floor (- (expt 10 50)) (+ (expt 10 25) 7)))
(multiple-value-list (ceiling (expt 10 50) (+ (expt 10 25) 7)))
(multiple-value-list (round (expt 10 50) (expt 10 25)))
(multiple-value-list (round (+ (expt 10 30) (* 5 (expt 10 9))) (expt 10 10)))
(multiple-value-list (round (+ (expt 10 30) (* 15 (expt 10 9))) (expt 10 10)))
(list (mod (- (fact 30)) 1000007) (rem (- (fact 30)) 1000007) (mod (fact 30) -1000007))
(multiple-value-list (floor (two-to 200) (- (two-to 70) 1)))
(multiple-value-list (truncate (- (two-to 300) 1) (two-to 150)))
(multiple-value-list (truncate (* (fact 40) (fact 40)) (+ (fact 40) 1)))
(multiple-value-list (truncate (* 340282366920938463463374607431768211457 3) 340282366920938463463374607431768211457))
(handler-case (truncate (fact 30) 0) (error () 'division-by-zero))

;; Number theory.
(list (gcd (fact 30) (fact 28)) (gcd (two-to 100) (expt 6 40)) (gcd) (gcd -12 18))
(list (lcm (two-to 70) (expt 3 50)) (lcm 4 6 10) (lcm))
(list (isqrt (expt 10 40)) (isqrt (- (expt 10 40) 1)) (isqrt (fact 30)) (isqrt 0) (isqrt 99))

;; Bits, on two's complement.
(list (ash 1 100) (ash (two-to 100) -98) (ash (- (two-to 100)) -98) (ash -1 -100) (ash 5 0))
(list (logand (two-to 100) (1- (two-to 100))) (logand (- (two-to 100)) (two-to 100)) (logand -1 (fact 30)))
(list (logior (two-to 70) 1) (logior (- (two-to 70)) 3) (logxor -1 (two-to 64)) (logxor (fact 30) (fact 30)))
(list (lognot (two-to 80)) (lognot (- (two-to 80))) (logand) (logior) (logxor 5 3 6))
(list (integer-length (two-to 100)) (integer-length (- (two-to 100))) (integer-length (1- (two-to 100))) (integer-length 0))
(list (logcount (1- (two-to 64))) (logcount (- (two-to 64))) (logbitp 100 (two-to 100)) (logbitp 99 (two-to 100))
      (logbitp 200 -1))

;; Reading and printing, in several radices.
123456789012345678901234567890
-98765432109876543210987654321
(list #x123456789ABCDEF0123456789 #b1000000000000000000000000000000000000000000000000000000000000000000)
(format nil "~D ~:D ~@D" (fact 25) (fact 25) (fact 25))
(format nil "~X ~O ~B" (two-to 70) (two-to 70) (two-to 70))
(format nil "~36R" (fact 30))
(write-to-string (fact 30) :base 16)
(prin1-to-string (- (expt 10 30)))
(read-from-string "-340282366920938463463374607431768211456")

;; Equality and hash tables: two equal bignums are different objects.
(list (eql (two-to 80) (two-to 80)) (equal (list (two-to 80)) (list (two-to 80))) (equalp (two-to 80) (two-to 80)))
(list (eql (two-to 80) (two-to 81)) (eql (two-to 80) 5) (member (two-to 80) (list 1 (two-to 80) 2)))
(let ((h (make-hash-table)))
  (setf (gethash (two-to 80) h) 'eighty)
  (setf (gethash (- (two-to 80)) h) 'minus)
  (list (gethash (* (two-to 40) (two-to 40)) h) (gethash (- (two-to 80)) h) (gethash (two-to 81) h)))
;; (A list as a key is refused by Quokka's hash tables altogether, #22.)
(let ((h (make-hash-table :test 'equal)))
  (setf (gethash (fact 30) h) 'equal-table)
  (gethash (* 30 (fact 29)) h))

;; Sequence functions meet bignums through eql.
(list (position (fact 30) (list 1 (fact 30))) (count (two-to 64) (list (two-to 64) (two-to 64) 3))
      (remove-duplicates (list (two-to 64) (two-to 64) 1)) (sort (list (fact 25) 3 (- (fact 22))) #'<))
(reduce #'* (list (fact 20) (fact 20) 2))
