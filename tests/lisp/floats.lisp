;;;; Double floats (#17, step M).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;;
;;;; Quokka has one float format, the double (#17, decision 2). SBCL reads
;;;; `1.5` as a single-float unless told otherwise, so the file tells it
;;;; first; every float below is then a double in both. Nothing here makes a
;;;; float from rationals alone — (sqrt 2), (float 1) — because SBCL makes a
;;;; single-float of that. The transcendental functions differ in the last
;;;; bit between maths libraries, so they are compared rounded (decision 7).
;;;; And no result may be complex: (sqrt -1) is a number in SBCL and an
;;;; error in Quokka.

(setf *read-default-float-format* 'double-float)

;; Reading, and printing the shortest digits that read back.
(list 1.5 -2.25 0.1 .5 -.5 +.5 1.e5 1e5 1.5d3 1.5e-3 -1.e-2 1E3)
(list 1. -12. 1.0)
(list 1d7 9999999.0 1d-3 9.99d-4 123456.789 1d23 5d-324 1d100 100.0 0.0 -0.0)
(list (+ 0.1 0.2) (* 1.1 1.1) (- 0.3 0.1) (/ 1.0 3) (/ 2.0 3))
(list most-positive-double-float most-negative-double-float
      least-positive-double-float least-positive-normalized-double-float)
(list double-float-epsilon double-float-negative-epsilon
      (= (+ 1d0 double-float-epsilon) 1d0) (= (+ 1d0 (/ double-float-epsilon 2)) 1d0))
(let ((xs (list 0.1 (/ 1.0 3) 1d23 5d-324 2.2250738585072014d-308 (sqrt 2d0) pi
                123456789.123 1.7976931348623157d308 -0.0 9007199254740993.0)))
  (every (lambda (x) (eql x (read-from-string (prin1-to-string x)))) xs))
(read-from-string "1d400")
(read-from-string "1d-400")
(list (prin1-to-string 1.5) (princ-to-string -0.0) (write-to-string 2.5 :base 16)
      (let ((*print-base* 2)) (prin1-to-string 0.75)))

;; Contagion: a rational meeting a float becomes a float first.
(list (+ 1 2.5) (- 10 0.5) (* 3 1.5) (/ 3 2.0) (/ 1.0 4) (+ 1 2 3.0) (- 1.5) (/ 2.0) (/ 4.0 2 2))
(list (/ 6 3) (/ 12 2 3) (/ -8 2))
(list (+ (expt 2 70) 1.5) (* (expt 10 20) 1d0) (- 1.0 (expt 2 64)) (/ (expt 2 80) 2d0)
      (+ 1.5 (expt 2 70) 1))
(list (float (expt 2 70) 1d0) (float (1+ (expt 2 53)) 1d0) (float (+ (expt 2 53) 3) 1d0)
      (float (- (expt 10 30)) 1d0) (float (+ (expt 2 70) (expt 2 17)) 1d0)
      (float (+ (expt 2 70) (expt 2 17) 1) 1d0) (float (+ (expt 2 70) (* 3 (expt 2 17))) 1d0))
(float (expt 10 400) 1d0)

;; Comparison is exact: a float against an integer as the rational it is.
(list (= 1 1.0) (< 1 1.5) (> 2 1.5) (= 0.0 -0.0) (< -0.0 0.0) (<= 1 1.0 2) (/= 1 1.0)
      (>= 2.5 2 2.0))
(list (max 1 2.0) (max 3 2.0) (min 1.5 2) (min -0.5 -1))
(list (= (1+ (expt 2 70)) (float (expt 2 70) 1d0)) (= (expt 2 70) (float (expt 2 70) 1d0))
      (> (1+ (expt 2 70)) (float (expt 2 70) 1d0)) (< (float (expt 2 70) 1d0) (1+ (expt 2 70)))
      (< (expt 2 70) (+ (float (expt 2 70) 1d0) 0.5)))
(list (= most-positive-fixnum (float most-positive-fixnum 1d0))
      (< most-positive-fixnum (float most-positive-fixnum 1d0))
      (> (float most-negative-fixnum 1d0) most-negative-fixnum)
      (= (float most-negative-fixnum 1d0) most-negative-fixnum))

;; eql by bits, = by value.
(list (eql 1.5 1.5) (eql 2.0 (+ 1.0 1.0)) (eql 0.0 -0.0) (eql 1 1.0) (equal 1.5 1.5)
      (equal 0.0 -0.0) (equalp 1 1.0) (equalp 0.0 -0.0) (equalp '(1 2.0) '(1.0 2)))
(list (member 2.0 '(1 2 2.0 3)) (position 2.5 '(1 2.5 3)) (assoc 1.5 '((1 . a) (1.5 . b))))
(let ((h (make-hash-table)))
  (setf (gethash 1.5 h) :a (gethash -0.0 h) :b)
  (list (gethash 1.5 h) (gethash (+ 1.0 0.5) h) (gethash 0.0 h) (gethash -0.0 h)))
(let ((h (make-hash-table :test 'equal)))
  (setf (gethash 2.5 h) :c)
  (list (gethash 2.5 h) (gethash 2 h) (gethash 2.50 h)))

;; Traps, as SBCL has them (decision 4); underflow is quiet.
(/ 1.0 0.0)
(/ 1 0.0)
(/ 0.0 0.0)
(* 1d308 10)
(+ most-positive-double-float most-positive-double-float)
(- most-negative-double-float 1d300)
(exp 1000d0)
(log 0d0)
(list (* 1d-300 1d-300) (/ least-positive-double-float 4))

;; Rounding.
(list (multiple-value-list (truncate 2.5)) (multiple-value-list (truncate -2.5))
      (multiple-value-list (floor 2.5)) (multiple-value-list (floor -2.5))
      (multiple-value-list (ceiling 2.5)) (multiple-value-list (ceiling -2.5))
      (multiple-value-list (round 2.5)) (multiple-value-list (round 3.5))
      (multiple-value-list (round -2.5)) (multiple-value-list (round -0.4)))
(list (multiple-value-list (floor 7.5 2)) (multiple-value-list (floor -7.5 2))
      (multiple-value-list (ceiling 7.5 2)) (multiple-value-list (round 7.5 2))
      (multiple-value-list (truncate -7.5 2)) (multiple-value-list (floor 7 2.0))
      (multiple-value-list (round 2.5 1)) (multiple-value-list (floor 1 0.25)))
(list (mod -7.5 2) (rem -7.5 2) (mod 7.5 -2) (rem 7.5 -2) (mod 5 1.5) (rem 10 2.5))
(list (truncate 1d20) (floor -1d20) (round 1.5d30) (ceiling 123456789012345678.0))
(list (ffloor 7.5 2) (fceiling 7.5 2) (ftruncate -0.5) (fround 2.5) (fround 3.5)
      (ffloor 7d0 2) (multiple-value-list (ffloor -7.5 2)))

;; Taking a float apart.
(list (multiple-value-list (integer-decode-float 1.5))
      (multiple-value-list (integer-decode-float -0.0))
      (multiple-value-list (integer-decode-float 5d-324))
      (multiple-value-list (integer-decode-float most-positive-double-float)))
(list (multiple-value-list (decode-float 1.5)) (multiple-value-list (decode-float -8.0))
      (multiple-value-list (decode-float 5d-324)) (multiple-value-list (decode-float 0.0)))
(list (scale-float 1.0 10) (scale-float 1.5 -3) (float-sign -2.0) (float-sign 0.0 -3.0)
      (float-sign -0.0) (float-digits 1.0) (float-precision 1.0) (float-precision 5d-324)
      (float-precision 0.0) (float-radix 1.0))

;; The other functions of CLHS 12.
(list (abs -2.5) (abs -0.0) (signum -2.5) (signum 0.0) (signum -0.0) (zerop -0.0)
      (plusp 0.5) (minusp -0.0) (1+ 1.5) (1- 0.5))
(list (floatp 1.5) (floatp 1) (numberp 1.5) (realp 1.5) (rationalp 1.5) (rationalp 1)
      (integerp 1.0))
(list (float 1 1d0) (float 1.5 1d0) (float -7 1d0) (coerce 3 'double-float)
      (coerce 2.5 'float))
(list (expt 2.0 10) (expt 2d0 -2) (expt 1.5 0) (expt 2d0 70) (expt 4d0 0.5d0) (expt -2.0 3))
(list (sqrt 2d0) (sqrt 16d0) (sqrt 0.25d0) (sqrt 1d-300) (sqrt 0d0))
(list (sin 0d0) (cos 0d0) (exp 0d0) (log 1d0) (atan 0d0) (tanh 0d0) (sinh 0d0) (asin 0d0))
(mapcar (lambda (x) (format nil "~,10F" x))
        (list (exp 1d0) (log 10d0) (sin 1d0) (cos 1d0) (tan 1d0) (asin 0.5d0) (acos 0.5d0)
              (atan 1d0) (atan 1d0 -2d0) (sinh 1d0) (cosh 1d0) (tanh 0.5d0) (log 8d0 2d0)
              (expt 2 0.5d0) (exp -1d0) (log 2.5d0)))
(format nil "~,12F" pi)

;; format's float directives.
(list (format nil "~F" 3.14159) (format nil "~F" -2.5) (format nil "~F" 1d-10)
      (format nil "~F" 1d20) (format nil "~F" 15) (format nil "~@F" 2.5))
(list (format nil "~,2F" 0.005) (format nil "~,2F" -0.005) (format nil "~,2F" 0.125)
      (format nil "~,2F" 2.675) (format nil "~,1F" 0.25) (format nil "~,0F" 0.5)
      (format nil "~,0F" -2.5) (format nil "~,3F" 1d10) (format nil "~,2F" 7))
(list (format nil "~8,3F" 3.14159) (format nil "~8,3F" -3.14159) (format nil "~3,2F" 0.5)
      (format nil "~4,2F" 0.5) (format nil "~2,2F" 123.456) (format nil "~2,2,,'*F" 123.456)
      (format nil "~8,2,,,'0F" 3.14159) (format nil "~,2,1F" 3.14159)
      (format nil "~,2,-1F" 3.14159) (format nil "~6F" 3.14159) (format nil "~6F" 123456.7))
(list (format nil "~E" 3.14159) (format nil "~E" 1d-10) (format nil "~E" 0.0)
      (format nil "~E" -1d100) (format nil "~,3E" 3.14159) (format nil "~,3E" 9.9996)
      (format nil "~,2,3E" 3.14159) (format nil "~12,3E" 3.14159)
      (format nil "~,3,,0E" 3.14159) (format nil "~,3,,2E" 3.14159) (format nil "~@E" 3.5)
      (format nil "~,2E" 1d-5))
(list (format nil "~G" 3.14159) (format nil "~G" 1d10) (format nil "~G" 1d-5)
      (format nil "~,2G" 3.14159) (format nil "~G" 0.0) (format nil "~G" 123456789.0))
(list (format nil "~$" 3.14159) (format nil "~$" -0.005) (format nil "~$" 1234.5)
      (format nil "~,3$" 2.5) (format nil "~,2,6$" 3.5) (format nil "~@$" 3.5)
      (format nil "~$" 0.5) (format nil "~$" 7) (format nil "~$" 1d20))
(format nil "~A ~S ~D ~5,1F|" 1.5 2.5 3.5 4.25)
