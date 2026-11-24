;;;; Ratios (#59, step AI of #39).
;;;;
;;;; In Common Lisp `(/ 1 3)` is neither 0 nor 0.333..., it is the exact
;;;; number one third, a *ratio*. A ratio and an integer together are the
;;;; *rationals*; with the floats they are the *reals* (CLHS 12.1.1).
;;;;
;;;; Decided in #59 (25 Nov 2026, under the standing guideline: do it the
;;;; right way now; when in doubt, as SBCL does):
;;;;
;;;; 1. Ratios are tier 1, with `/` on integers, `numerator`, `denominator`,
;;;;    `rational`, `rationalize` and the `1/3` syntax, as SBCL has them.
;;;; 2. A ratio is a heap object of two slots, the numerator and the
;;;;    denominator (`crate::ratio`), always in lowest terms with a
;;;;    denominator above 1: *rational canonicalisation* (CLHS 12.1.3.2),
;;;;    done by `%ratio` here before anything is made. So `(/ 4 2)` is the
;;;;    integer 2, and two ratios are `eql` when their parts are.
;;;; 3. The algorithms are Lisp, as for bignums (#16): a primitive that meets
;;;;    a ratio hands over to the `%generic-` functions in `integers.lisp`,
;;;;    which call the functions below.

;;; ------------------------------------------------------------------------
;;; Making one

;;; N/D as a rational in lowest terms: the integer itself when D divides N,
;;; otherwise a ratio with a positive denominator.
(defun %ratio (n d)
  (if (eql d 0) (error 'division-by-zero "division by zero" n))
  (let ((g (gcd n d)))
    (let ((n (truncate n g))
          (d (truncate d g)))
      (if (minusp d) (setq n (- n) d (- d)))
      (if (eql d 1) n (%make-ratio n d)))))

(defun numerator (rational)
  (cond ((%ratiop rational) (%ratio-numerator rational))
        ((integerp rational) rational)
        (t (error 'type-error "numerator wants a rational" rational))))

(defun denominator (rational)
  (cond ((%ratiop rational) (%ratio-denominator rational))
        ((integerp rational) 1)
        (t (error 'type-error "denominator wants a rational" rational))))


;;; ------------------------------------------------------------------------
;;; Arithmetic, for the fallbacks

;;; A rational operation on A and B when either is a ratio, or nil: each is
;;; a numerator over a denominator, an integer over 1, and the result is put
;;; in lowest terms by `%ratio`.
(defun %ratio-arithmetic (operation a b)
  (if (or (%ratiop a) (%ratiop b))
      (let ((an (numerator a)) (ad (denominator a))
            (bn (numerator b)) (bd (denominator b)))
        (cond ((eq operation '+) (%ratio (+ (* an bd) (* bn ad)) (* ad bd)))
              ((eq operation '-) (%ratio (- (* an bd) (* bn ad)) (* ad bd)))
              ((eq operation '*) (%ratio (* an bn) (* ad bd)))
              (t (%ratio (* an bd) (* ad bn)))))
      nil))

;;; -1, 0 or 1, comparing two rationals at least one of which is a ratio:
;;; the cross products, since both denominators are positive.
(defun %compare-rationals (a b)
  (let ((left (* (numerator a) (denominator b)))
        (right (* (numerator b) (denominator a))))
    (cond ((< left right) -1) ((> left right) 1) (t 0))))

;;; `truncate` with a ratio among NUMBER and DIVISOR (both rational): the
;;; quotient of the exact quotient's parts, and what is left.
(defun %truncate-rational (number divisor)
  (let* ((exact (/ number divisor))
         (quotient (%quotient (numerator exact) (denominator exact))))
    (values quotient (- number (* quotient divisor)))))

;;; ------------------------------------------------------------------------
;;; Ratios and floats

;;; The nearest double to a ratio, ties to even, as `float` makes it. The
;;; quotient of the magnitudes is taken to 55 bits or more, and whatever
;;; division leaves over is kept as a *sticky* bit, so the rounding sees
;;; the whole remainder: more than half, exactly half, or less. A quotient
;;; that falls below the normal range is rounded again where it lands, a
;;; double rounding SBCL avoids; it can differ from SBCL's in the last bit of
;;; a subnormal.
(defun %ratio-to-double (ratio)
  (let* ((n (abs (numerator ratio)))
         (d (denominator ratio))
         (shift (- 55 (- (integer-length n) (integer-length d))))
         (scaled (if (plusp shift) (ash n shift) n))
         (divisor (if (plusp shift) d (ash d (- shift)))))
    (multiple-value-bind (q r) (truncate scaled divisor)
      (let* ((extra (- (integer-length q) 53))
             (top (ash q (- extra)))
             (dropped (- q (ash top extra)))
             (half (ash 1 (- extra 1)))
             (above-half (or (> dropped half) (and (= dropped half) (not (zerop r))))))
        (if (or above-half (and (= dropped half) (zerop r) (oddp top)))
            (setq top (+ top 1)))
        (let ((double (%scale-double (%float top) (- extra shift))))
          (if (minusp (numerator ratio)) (- double) double))))))

;;; `(rational number)` — exactly the rational a float is: its mantissa
;;; over a power of two (CLHS `rational`). A rational is itself.
(defun rational (number)
  (cond ((rationalp number) number)
        ((floatp number)
         (destructuring-bind (mantissa exponent sign) (%decode-double number)
           (if (minusp exponent)
               (%ratio (* sign mantissa) (ash 1 (- exponent)))
               (* sign (ash mantissa exponent)))))
        (t (error 'type-error "rational wants a real number" number))))

;;; `(rationalize number)` — the simplest rational that reads back as the
;;; same float: the one with the smallest denominator between the float's
;;; neighbours halfway down and halfway up (SBCL's algorithm, after Bill
;;; Gosper: a continued fraction walk down the interval).
(defun rationalize (number)
  (cond ((rationalp number) number)
        ((floatp number)
         (if (zerop number)
             0
             (destructuring-bind (mantissa exponent sign) (%decode-double number)
               (let* ((low (%ratio (- (* 2 mantissa) 1) (ash 1 (- 1 exponent))))
                      (high (%ratio (+ (* 2 mantissa) 1) (ash 1 (- 1 exponent)))))
                 (* sign (%simplest-between low high))))))
        (t (error 'type-error "rationalize wants a real number" number))))

;;; The rational with the smallest denominator in [LOW, HIGH], 0 < LOW <
;;; HIGH: an integer if one is there, or else the integer part of both and
;;; the reciprocal of the simplest rational between the reciprocals of what
;;; is left.
(defun %simplest-between (low high)
  (let ((whole (floor low)))
    (cond ((= whole low) whole)
          ((< whole (floor high)) (+ whole 1))
          (t (+ whole (/ 1 (%simplest-between (/ 1 (- high whole))
                                              (/ 1 (- low whole)))))))))
