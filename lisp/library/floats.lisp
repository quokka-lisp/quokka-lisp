;;;; Double floats (#17, step M of #39).
;;;;
;;;; A float is a number with a binary point that floats: a 53-bit integer
;;;; scaled by a power of two, which is how IEEE 754 stores a double. Rust
;;;; does the arithmetic (`crate::float`); this file is everything above it.
;;;;
;;;; Decided in #17 (Carsten):
;;;;
;;;; 1. A double is a boxed heap object of one raw slot.
;;;; 2. Doubles only: CL's four float types are all the double (CLHS 12.1.4
;;;;    permits it), so `1.5` reads as a double — where SBCL reads a 32-bit
;;;;    single-float. Differential files bind `*read-default-float-format*`
;;;;    to `double-float` to make SBCL agree.
;;;; 3. The fastest arithmetic: `+ - * / = < >` and `rem` take doubles in
;;;;    Rust. Only a bignum meeting a double comes here, through the
;;;;    fallback functions in `integers.lisp`, which call `%contagion` and
;;;;    `%compare-real` below.
;;;; 4. Traps, as SBCL has them: an infinity or a NaN is an error, so no
;;;;    Lisp code ever holds one.
;;;; 5. Rust converts between floats and text; laying the digits out is Lisp
;;;;    (the printer, and `format`'s ~F ~E ~G ~$).
;;;; 6. The scope below; ratios, `rational` and `rationalize` wait for #59.
;;;; 7. The transcendental functions come from the `libm` crate on every
;;;;    target, so an image computes the same bits natively and in a browser.
;;;;
;;;; No float is written as a literal in this file: the library is read at
;;;; build time by the Rust reader, which knows no floats. Constants are
;;;; parsed from strings instead.

;;; ------------------------------------------------------------------------
;;; Converting

;;; `(float number)` — the nearest double. A fixnum is Rust's to convert; a
;;; bignum is too large for that, and is rounded here. The prototype says
;;; which float format, and there is only one.
(defun float (number &optional prototype)
  prototype
  (cond ((floatp number) number)
        ((%bignump number) (%integer-to-double number))
        ((integerp number) (%float number))
        (t (error "FLOAT wants a real number" number))))

;;; A bignum as the nearest double, ties to even — as Rust's `as` rounds a
;;; fixnum. Keep the top 53 bits; the bits shifted out decide whether to
;;; round up: more than half of the last kept bit's worth, or exactly half
;;; and the kept bits odd. A carry out of 53 bits (2^53) is still exact.
(defun %integer-to-double (integer)
  (let* ((magnitude (abs integer))
         (shift (- (integer-length magnitude) 53))
         (top (ash magnitude (- shift)))
         (dropped (- magnitude (ash top shift)))
         (half (ash 1 (- shift 1))))
    (if (or (> dropped half) (and (= dropped half) (oddp top)))
        (setq top (+ top 1)))
    (let ((double (%scale-double (%float top) shift)))
      (if (minusp integer) (- double) double))))

;;; An integral double as the integer it is: a fixnum from Rust when it
;;; fits, otherwise its mantissa shifted by its exponent.
(defun %float-integer (double)
  (or (%double-integer double)
      (destructuring-bind (mantissa exponent sign) (%decode-double double)
        (* sign (ash mantissa exponent)))))

;;; ------------------------------------------------------------------------
;;; Mixed arithmetic, for the fallbacks

;;; Float contagion (CLHS 12.1.4.1), for a bignum meeting a double: both
;;; become doubles and FUNCTION — `+` or the like, which Rust does on two
;;; doubles — combines them. Nil if neither is a float.
(defun %contagion (function a b)
  (if (or (floatp a) (floatp b))
      (funcall function (float a) (float b))
      nil))

;;; -1, 0 or 1 as A is less than, equal to or greater than B — exactly, as
;;; CLHS 12.1.4.1 requires: a float compared with a rational is compared as
;;; the rational it is. Converting the bignum to a double instead would make
;;; (= (1+ (expt 2 70)) (float (expt 2 70))) true.
(defun %compare-real (a b)
  (cond ((and (integerp a) (integerp b)) (%integer-compare a b))
        ((and (floatp a) (floatp b)) (cond ((< a b) -1) ((> a b) 1) (t 0)))
        ((floatp a) (- (%compare-integer-float b a)))
        (t (%compare-integer-float a b))))

;;; An integer against a double: against the double's floor, an integer;
;;; on a tie the double is either that integer or a little more.
(defun %compare-integer-float (integer double)
  (let* ((down (%double-round double 1))
         (floor (%float-integer down)))
    (cond ((< integer floor) -1)
          ((> integer floor) 1)
          ((= double down) 0)
          (t -1))))

;;; `truncate` of two reals at least one of which is a float, as SBCL does
;;; it: divide, truncate the quotient, and the remainder is what is left.
;;; `floor`, `ceiling` and `round` adjust this, in `numbers.lisp` and
;;; `integers.lisp`, exactly as they adjust an integer truncation.
(defun %truncate-float (number divisor)
  (let ((quotient (%double-round (/ number divisor) 0)))
    (values (%float-integer quotient) (- number (* quotient divisor)))))

;;; `ftruncate`, `ffloor`, `fceiling`, `fround`: the quotient as a float.
;;; With a float, the quotient is rounded as a double in MODE (0 towards
;;; zero, 1 down, 2 up, 3 to even), which keeps the sign of a zero:
;;; (ftruncate -0.5) is -0.0. With two integers, it is FUNCTION's quotient,
;;; made a float.
(defun %float-rounding (function mode number divisor)
  (if (or (floatp number) (floatp divisor))
      (let ((quotient (%double-round (/ number divisor) mode)))
        (values quotient (- number (* quotient divisor))))
      (multiple-value-bind (quotient remainder) (funcall function number divisor)
        (values (float quotient) remainder))))

(defun ftruncate (number &optional (divisor 1)) (%float-rounding #'truncate 0 number divisor))
(defun ffloor (number &optional (divisor 1)) (%float-rounding #'floor 1 number divisor))
(defun fceiling (number &optional (divisor 1)) (%float-rounding #'ceiling 2 number divisor))
(defun fround (number &optional (divisor 1)) (%float-rounding #'round 3 number divisor))

;;; ------------------------------------------------------------------------
;;; Taking a float apart (CLHS 12.2)

(defun float-radix (float) float 2)
(defun float-digits (float) float 53)

;;; The mantissa, exponent and sign, the double being exactly their product
;;; with 2 raised to the exponent. Rust reads them out of the bits.
(defun integer-decode-float (float)
  (values-list (%decode-double float)))

;;; How many of the 53 bits are significant: all of them, except in a
;;; subnormal — a number too small for the normal range, which gives up
;;; leading bits to go lower — and in zero, which has none.
(defun float-precision (float)
  (integer-length (car (%decode-double float))))

;;; A significand in [1/2, 1), an exponent and a sign, the double being
;;; their product. A subnormal's mantissa is shifted up first, so its
;;; significand is in range too, as SBCL's is.
(defun decode-float (float)
  (destructuring-bind (mantissa exponent sign) (%decode-double float)
    (if (zerop mantissa)
        (values (float 0) 0 (float sign))
        (let ((shift (- 53 (integer-length mantissa))))
          (values (%scale-double (%float (ash mantissa shift)) -53)
                  (+ exponent 53 (- shift))
                  (float sign))))))

(defun scale-float (float integer) (%scale-double float integer))

;;; |FLOAT-2| with the sign of FLOAT — which, unlike `minusp`, tells -0.0
;;; from 0.0.
(defun float-sign (float-1 &optional (float-2 (float 1)))
  (let ((magnitude (if (%negative-float-p float-2) (- float-2) float-2)))
    (if (%negative-float-p float-1) (- magnitude) magnitude)))

(defun %negative-float-p (float)
  (eq (third (%decode-double float)) -1))

;;; ------------------------------------------------------------------------
;;; Irrational and transcendental functions (CLHS 12.2)
;;;
;;; Rust computes them (`libm`, decision 7). A result that would be complex
;;; — (sqrt -1), (log -1), (asin 2) — is an error: Quokka has no complex
;;; numbers. Each takes any real, made a double first.

(defun %real-argument (number)
  (if (numberp number) (float number) (error "a real number is wanted" number)))

(defun %not-complex (name number)
  (if (minusp number)
      (error "the result would be a complex number, which Quokka does not have" name number)))

(defun sqrt (number) (%not-complex 'sqrt number) (%sqrt (%real-argument number)))
(defun exp (number) (%exp (%real-argument number)))

;;; With a BASE, the quotient of two natural logarithms, as SBCL computes it.
(defun log (number &optional (base nil base-p))
  (%not-complex 'log number)
  (if base-p
      (/ (log number) (log base))
      (%log (%real-argument number))))

(defun sin (radians) (%sin (%real-argument radians)))
(defun cos (radians) (%cos (%real-argument radians)))
(defun tan (radians) (%tan (%real-argument radians)))
(defun asin (number) (%asin (%real-argument number)))
(defun acos (number) (%acos (%real-argument number)))

;;; One argument, the angle whose tangent it is; two, the angle of the point
;;; (X, Y) — `atan2`, which knows which quadrant the point is in.
(defun atan (y &optional (x nil x-p))
  (if x-p
      (%atan2 (%real-argument y) (%real-argument x))
      (%atan (%real-argument y))))

(defun sinh (number) (%sinh (%real-argument number)))
(defun cosh (number) (%cosh (%real-argument number)))
(defun tanh (number) (%tanh (%real-argument number)))

;;; The inverses (#68). Outside their real domains — `acosh` below 1,
;;; `atanh` beyond ±1 — CL answers a complex number, which Quokka defers;
;;; here, as for `sqrt` of a negative, it is an error.
(defun asinh (number) (%asinh (%real-argument number)))
(defun acosh (number) (%acosh (%real-argument number)))
(defun atanh (number) (%atanh (%real-argument number)))

;;; `expt` with a float power: Rust's `pow`. An integer power goes by
;;; repeated squaring in `integers.lisp`, as SBCL's does, which is why
;;; (expt 2d0 70) is exact.
(defun %expt-float (base power)
  (%not-complex 'expt base)
  (%pow (%real-argument base) (%real-argument power)))

;;; ------------------------------------------------------------------------
;;; Constants (CLHS 12.2)

;;; What the reader makes of `1.5`, and the format the printer leaves
;;; unmarked. CL's initial value, though it names the same type as every
;;; other float format here (decision 2): `1.5` is a double either way.
(defvar *read-default-float-format* 'single-float)

(defvar pi (%parse-double "3.141592653589793"))
(defvar most-positive-double-float (%parse-double "1.7976931348623157e308"))
(defvar most-negative-double-float (- most-positive-double-float))
(defvar least-positive-double-float (%parse-double "4.9406564584124654e-324"))
(defvar least-negative-double-float (- least-positive-double-float))
(defvar least-positive-normalized-double-float (%parse-double "2.2250738585072014e-308"))
(defvar least-negative-normalized-double-float (- least-positive-normalized-double-float))
;;; The smallest number that changes 1.0 when added to it, as SBCL has it:
;;; 2^-53 and a little more, so that the sum rounds up rather than to even.
(defvar double-float-epsilon (%parse-double "1.1102230246251568e-16"))
(defvar double-float-negative-epsilon (%parse-double "5.551115123125784e-17"))

;;; ------------------------------------------------------------------------
;;; Digits, for the printer and `format`

;;; The shortest digits that read back as |DOUBLE|, as a string, and the
;;; power of ten of the first: 123.5 is "1235" and 2. Rust finds them
;;; (decision 5) and writes them as `1.235e2`; here they are taken apart.
(defun %float-digits (double)
  (let* ((text (%double-digits double))
         (e (%position-of-char #\e text 0 (length text)))
         (digits (remove #\. (subseq text 0 e))))
    (values digits (%token-integer (subseq text (+ e 1)) 10))))

;;; |DOUBLE| × 10^PLACES, rounded to an integer, halves up — exactly, from
;;; the double's own bits, as SBCL's `format` rounds: (format nil "~,1F"
;;; 0.25) is "0.3", and 2.675, which is really 2.67499999..., gives "2.67".
(defun %scaled-integer (double places)
  (destructuring-bind (mantissa exponent sign) (%decode-double double)
    sign
    (let ((numerator (* mantissa (ash 1 (max exponent 0)) (expt 10 (max places 0))))
          (denominator (* (ash 1 (max (- exponent) 0)) (expt 10 (max (- places) 0)))))
      (multiple-value-bind (quotient remainder) (floor numerator denominator)
        (if (>= (* 2 remainder) denominator) (+ quotient 1) quotient)))))

;;; DIGITS with a decimal point PLACES from the right, zeros added in front
;;; as needed: "5" and 2 are "0.05". PLACES of zero leaves "5.".
(defun %point-from-right (digits places)
  (let ((digits (if (> (+ places 1) (length digits))
                    (concatenate 'string
                                 (make-string (- (+ places 1) (length digits))
                                              :initial-element #\0)
                                 digits)
                    digits)))
    (concatenate 'string
                 (subseq digits 0 (- (length digits) places))
                 "."
                 (subseq digits (- (length digits) places)))))

;;; |DOUBLE| in positional notation, with every shortest digit: 1d20 is
;;; "100000000000000000000.0" and 1.5d-5 "0.000015", as SBCL's ~F writes
;;; them. The printer uses it between 10^-3 and 10^7.
(defun %fixed-shortest (double)
  (if (zerop double)
      "0.0"
      (multiple-value-bind (digits exponent) (%float-digits double)
        (let ((places (max 1 (- (length digits) exponent 1))))
          (%point-from-right
           (concatenate 'string digits
                        (make-string (max 0 (- places (- (length digits) exponent 1)))
                                     :initial-element #\0))
           places)))))
