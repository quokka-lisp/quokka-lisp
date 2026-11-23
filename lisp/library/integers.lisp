;;;; Integers of any size (#16, step L of #39).
;;;;
;;;; A fixnum holds 63 bits. When `+`, `-`, `*`, `=`, `<`, `>`, `%quotient`
;;;; or `rem` overflow, or meet a bignum, the machine calls the matching
;;;; `%generic-` function here instead (decision 1): the fast path in Rust is
;;;; untouched, and only the slow path is Lisp. Rust keeps the representation
;;;; and what is bounded by it (`crates/quokka-vm/src/bignum.rs`); the
;;;; algorithms are here, so a long multiplication is sliced like any Lisp.
;;;;
;;;; A bignum is a sign and a magnitude in limbs of 30 bits, least
;;;; significant first (decision 2, as CPython's `int`). Thirty, so that the
;;;; loops below can do their work on fixnums: a limb times a limb, plus a
;;;; carry, is at most 61 bits, and a fixnum has 62 bits of magnitude. Every
;;;; intermediate value in this file is a fixnum, so nothing here can call
;;;; itself through the fallback.
;;;;
;;;; The working form is a **magnitude**: a simple vector of limbs with no
;;;; leading zero (zero is the empty vector). An integer is taken apart into
;;;; a sign and a magnitude, the work is done on magnitudes, and the result is
;;;; put back together and normalised — a fixnum whenever it fits.

(defvar %limb-base 1073741824)          ; 2^30
(defvar %limb-mask 1073741823)          ; 2^30 - 1

(defvar most-positive-fixnum 4611686018427387903)
(defvar most-negative-fixnum -4611686018427387904)

;;; ------------------------------------------------------------------------
;;; Taking integers apart and putting them back

;;; N as two values: whether it is negative, and its magnitude.
(defun %magnitude (n)
  (let* ((b (if (%bignump n) n (%fixnum-to-bignum n)))
         (length (%bignum-length b))
         (limbs (make-array length)))
    (dotimes (i length)
      (setf (aref limbs i) (%bignum-limb b i)))
    (let ((m (%mag-trim limbs)))
      (values (and (%bignum-negative-p b) (> (length m) 0)) m))))

;;; The integer with this sign and magnitude, normalised.
(defun %make-integer (negative magnitude)
  (let ((b (%make-bignum negative (length magnitude))))
    (dotimes (i (length magnitude))
      (%set-bignum-limb b i (aref magnitude i)))
    (%bignum-normalize b)))

;;; MAGNITUDE without its leading zero limbs.
(defun %mag-trim (magnitude)
  (let ((n (length magnitude)))
    (do () ((or (= n 0) (not (= (aref magnitude (- n 1)) 0))))
      (setq n (- n 1)))
    (if (= n (length magnitude)) magnitude (subseq magnitude 0 n))))

(defun %limb-high (x) (%quotient x %limb-base))
(defun %limb-low (x) (%fixnum-logand x %limb-mask))

;;; ------------------------------------------------------------------------
;;; Magnitudes

(defun %mag-compare (a b)
  (cond ((< (length a) (length b)) -1)
        ((> (length a) (length b)) 1)
        (t (do ((i (- (length a) 1) (- i 1)))
               ((< i 0) 0)
             (cond ((< (aref a i) (aref b i)) (return -1))
                   ((> (aref a i) (aref b i)) (return 1)))))))

;;; Schoolbook addition, one limb at a time with a carry.
(defun %mag-add (a b)
  (let* ((n (max (length a) (length b)))
         (out (make-array (+ n 1)))
         (carry 0))
    (dotimes (i n)
      (let ((sum (+ (if (< i (length a)) (aref a i) 0)
                    (if (< i (length b)) (aref b i) 0)
                    carry)))
        (setf (aref out i) (%limb-low sum))
        (setq carry (%limb-high sum))))
    (setf (aref out n) carry)
    (%mag-trim out)))

;;; A - B, where A is at least B: subtraction with a borrow.
(defun %mag-sub (a b)
  (let ((out (make-array (length a)))
        (borrow 0))
    (dotimes (i (length a))
      (let ((d (- (aref a i) (if (< i (length b)) (aref b i) 0) borrow)))
        (if (< d 0)
            (progn (setf (aref out i) (+ d %limb-base)) (setq borrow 1))
            (progn (setf (aref out i) d) (setq borrow 0)))))
    (%mag-trim out)))

;;; Schoolbook multiplication: every limb of A times every limb of B, each
;;; product added into its column with the carry. O(n*m).
(defun %mag-mul (a b)
  (if (or (= (length a) 0) (= (length b) 0))
      (vector)
      (let ((out (make-array (+ (length a) (length b)) :initial-element 0)))
        (dotimes (i (length a))
          (let ((carry 0) (ai (aref a i)))
            (dotimes (j (length b))
              (let ((x (+ (* ai (aref b j)) (aref out (+ i j)) carry)))
                (setf (aref out (+ i j)) (%limb-low x))
                (setq carry (%limb-high x))))
            (setf (aref out (+ i (length b))) carry)))
        (%mag-trim out))))

;;; MAGNITUDE times 2^SHIFT, for SHIFT below 30, with one limb to spare.
(defun %mag-shift-left (magnitude shift extra)
  (let ((out (make-array (+ (length magnitude) extra) :initial-element 0))
        (factor (%expt-fixnum 2 shift))
        (carry 0))
    (dotimes (i (length magnitude))
      (let ((x (+ (* (aref magnitude i) factor) carry)))
        (setf (aref out i) (%limb-low x))
        (setq carry (%limb-high x))))
    (if (> extra 0) (setf (aref out (length magnitude)) carry))
    out))

;;; MAGNITUDE divided by 2^SHIFT, for SHIFT below 30.
(defun %mag-shift-right (magnitude shift)
  (let ((out (make-array (length magnitude)))
        (divisor (%expt-fixnum 2 shift))
        (carry 0))
    (do ((i (- (length magnitude) 1) (- i 1))) ((< i 0))
      (let ((x (+ (* carry %limb-base) (aref magnitude i))))
        (setf (aref out i) (%quotient x divisor))
        (setq carry (rem x divisor))))
    (%mag-trim out)))

(defun %expt-fixnum (base power)
  (let ((r 1)) (dotimes (i power r) (setq r (* r base)))))

;;; Divide by one limb, from the top, the way division is done by hand.
(defun %mag-divide-limb (a d)
  (let ((q (make-array (length a))) (r 0))
    (do ((i (- (length a) 1) (- i 1))) ((< i 0))
      (let ((x (+ (* r %limb-base) (aref a i))))
        (setf (aref q i) (%quotient x d))
        (setq r (rem x d))))
    (values (%mag-trim q) r)))

;;; Quotient and remainder of two magnitudes: Knuth's algorithm D (TAOCP
;;; volume 2, 4.3.1). Long division, one limb of the quotient at a time; each
;;; quotient limb is *estimated* from the top two limbs of what is left and
;;; the top limb of the divisor, then corrected. Shifting both numbers first
;;; so the divisor's top limb has its high bit set makes the estimate at most
;;; two too large, and the inner test usually fixes even that.
(defun %mag-divide (u v)
  (cond ((< (%mag-compare u v) 0) (values (vector) u))
        ((= (length v) 1)
         (multiple-value-bind (q r) (%mag-divide-limb u (aref v 0))
           (values q (if (= r 0) (vector) (vector r)))))
        (t (%knuth-d u v))))

(defun %knuth-d (u v)
  (let* ((n (length v))
         (m (- (length u) n))
         (shift (%normalizing-shift (aref v (- n 1))))
         (un (%mag-shift-left u shift 1))
         (vn (%mag-shift-left v shift 0))
         (q (make-array (+ m 1) :initial-element 0))
         (top (aref vn (- n 1)))
         (next (aref vn (- n 2))))
    (do ((j m (- j 1))) ((< j 0))
      ;; Estimate this quotient limb from the top two limbs.
      (let* ((num (+ (* (aref un (+ j n)) %limb-base) (aref un (+ j n -1))))
             (qhat (%quotient num top))
             (rhat (- num (* qhat top))))
        (do () ((not (and (< rhat %limb-base)
                          (or (>= qhat %limb-base)
                              (> (* qhat next)
                                 (+ (* rhat %limb-base) (aref un (+ j n -2))))))))
          (setq qhat (- qhat 1))
          (setq rhat (+ rhat top)))
        ;; Subtract QHAT times the divisor from the top of what is left.
        (let ((borrow 0) (carry 0))
          (dotimes (i n)
            (let* ((p (+ (* qhat (aref vn i)) carry))
                   (d (- (aref un (+ i j)) (%limb-low p) borrow)))
              (setq carry (%limb-high p))
              (if (< d 0)
                  (progn (setf (aref un (+ i j)) (+ d %limb-base)) (setq borrow 1))
                  (progn (setf (aref un (+ i j)) d) (setq borrow 0)))))
          (let ((d (- (aref un (+ j n)) carry borrow)))
            (if (>= d 0)
                (setf (aref un (+ j n)) d)
                ;; QHAT was one too many, which happens rarely: add the
                ;; divisor back once.
                (let ((carry 0))
                  (setf (aref un (+ j n)) (+ d %limb-base))
                  (setq qhat (- qhat 1))
                  (dotimes (i n)
                    (let ((s (+ (aref un (+ i j)) (aref vn i) carry)))
                      (setf (aref un (+ i j)) (%limb-low s))
                      (setq carry (%limb-high s))))
                  (setf (aref un (+ j n)) (%limb-low (+ (aref un (+ j n)) carry)))))))
        (setf (aref q j) qhat)))
    (values (%mag-trim q) (%mag-shift-right (subseq un 0 n) shift))))

;;; How far to shift a top limb left so its highest bit (bit 29) is set.
(defun %normalizing-shift (limb)
  (let ((shift 0))
    (do () ((>= limb 536870912) shift)          ; 2^29
      (setq limb (* limb 2))
      (setq shift (+ shift 1)))))

;;; ------------------------------------------------------------------------
;;; The generic operations the machine falls back to

(defun %add2 (a b)
  (multiple-value-bind (an am) (%magnitude a)
    (multiple-value-bind (bn bm) (%magnitude b)
      (cond ((eq an bn) (%make-integer an (%mag-add am bm)))
            ((>= (%mag-compare am bm) 0) (%make-integer an (%mag-sub am bm)))
            (t (%make-integer bn (%mag-sub bm am)))))))

(defun %negate (n)
  (multiple-value-bind (negative m) (%magnitude n)
    (%make-integer (and (not negative) (> (length m) 0)) m)))

(defun %mul2 (a b)
  (multiple-value-bind (an am) (%magnitude a)
    (multiple-value-bind (bn bm) (%magnitude b)
      (%make-integer (not (eq an bn)) (%mag-mul am bm)))))

;;; Truncating division: the quotient rounded towards zero, and the
;;; remainder with the sign of the dividend — CL's `truncate` and `rem`.
(defun %divide2 (a b)
  (multiple-value-bind (an am) (%magnitude a)
    (multiple-value-bind (bn bm) (%magnitude b)
      (if (= (length bm) 0) (error "division by zero" a))
      (multiple-value-bind (q r) (%mag-divide am bm)
        (values (%make-integer (not (eq an bn)) q)
                (%make-integer an r))))))

;;; Two at a time from the left, as SBCL combines them. A double among
;;; bignums makes both doubles first (`%contagion`, in `floats.lisp`, step
;;; M); the Rust primitives do every other mix of fixnums and doubles.
(defun %generic-+ (&rest numbers)
  (if (null numbers)
      0
      (let ((sum (car numbers)))
        (dolist (n (cdr numbers) sum)
          (setq sum (or (%contagion #'+ sum n) (%add2 sum n)))))))

(defun %generic-- (number &rest more)
  (if (null more)
      (%negate number)
      (let ((difference number))
        (dolist (n more difference)
          (setq difference (or (%contagion #'- difference n) (%add2 difference (%negate n))))))))

(defun %generic-* (&rest numbers)
  (if (null numbers)
      1
      (let ((product (car numbers)))
        (dolist (n (cdr numbers) product)
          (setq product (or (%contagion #'* product n) (%mul2 product n)))))))

;;; `/` arrives with floats (step M). Between integers it is exact or it is
;;; a ratio, and ratios do not exist yet (#59).
(defun %generic-/ (number &rest more)
  (if (null more)
      (%divide-real 1 number)
      (let ((quotient number))
        (dolist (n more quotient) (setq quotient (%divide-real quotient n))))))

(defun %divide-real (a b)
  (or (%contagion #'/ a b)
      (multiple-value-bind (quotient remainder) (%divide2 a b)
        (if (zerop remainder)
            quotient
            (error "/ would make a ratio, and ratios do not exist yet (#59)" a b)))))

(defun %compare-chain (test numbers)
  (do ((rest numbers (cdr rest)))
      ((null (cdr rest)) t)
    (if (not (funcall test (%compare-real (car rest) (car (cdr rest))))) (return nil))))

(defun %generic-= (number &rest more)
  (%compare-chain (lambda (order) (= order 0)) (cons number more)))
(defun %generic-< (number &rest more)
  (%compare-chain (lambda (order) (= order -1)) (cons number more)))
(defun %generic-> (number &rest more)
  (%compare-chain (lambda (order) (= order 1)) (cons number more)))

(defun %generic-quotient (a b)
  (if (or (floatp a) (floatp b)) (nth-value 0 (truncate a b)) (nth-value 0 (%divide2 a b))))
(defun %generic-rem (a b)
  (or (%contagion #'rem a b) (nth-value 1 (%divide2 a b))))

;;; ------------------------------------------------------------------------
;;; More of CLHS chapter 12

;;; Integers and floats; ratios are #59, and complex numbers are not
;;; planned.
(defun numberp (object) (or (integerp object) (floatp object)))
(defun realp (object) (numberp object))
(defun rationalp (object) (integerp object))

;;; A float's sign is a float, and a zero is itself: (signum -0.0) is -0.0.
(defun signum (number)
  (cond ((floatp number) (if (zerop number) number (float-sign number)))
        ((plusp number) 1) ((minusp number) -1) (t 0)))

;;; `ceiling` and `round` from `truncate`: the quotient moved one step when
;;; the remainder says truncation went the wrong way.
(defun ceiling (number &optional (divisor 1))
  (multiple-value-bind (q r) (truncate number divisor)
    (if (and (not (zerop r)) (eq (minusp number) (minusp divisor)))
        (values (+ q 1) (- r divisor))
        (values q r))))

;;; To the nearest integer, and to the even one on a tie.
(defun round (number &optional (divisor 1))
  (multiple-value-bind (q r) (truncate number divisor)
    (let ((twice (abs (* 2 r))) (d (abs divisor)))
      (if (or (> twice d) (and (= twice d) (oddp q)))
          (if (eq (minusp number) (minusp divisor))
              (values (+ q 1) (- r divisor))
              (values (- q 1) (+ r divisor)))
          (values q r)))))

(defun gcd (&rest integers)
  (let ((g 0))
    (dolist (n integers g) (setq g (%gcd2 g (abs n))))))

(defun %gcd2 (a b)
  (if (zerop b) a (%gcd2 b (rem a b))))

(defun lcm (&rest integers)
  (if (null integers)
      1
      (let ((l (abs (car integers))))
        (dolist (n (cdr integers) l)
          (setq l (if (or (zerop l) (zerop n))
                      0
                      (truncate (abs (* l n)) (%gcd2 l (abs n)))))))))

;;; Square and multiply: the bits of POWER, from the bottom, each squaring
;;; the base. A negative power of an integer would make a ratio (#59); of a
;;; float, it is the reciprocal. A float power is Rust's `pow` (step M).
(defun expt (base power)
  (cond ((floatp power) (%expt-float base power))
        ((and (minusp power) (floatp base)) (/ (expt base (- power))))
        ((minusp power)
         (error "EXPT: a negative power makes a ratio, which Quokka has not got (#59)" power))
        (t (%expt-integer base power))))

(defun %expt-integer (base power)
  (let ((result (if (floatp base) (float 1) 1)))
    (do () ((zerop power) result)
      (if (oddp power) (setq result (* result base)))
      (setq power (truncate power 2))
      (if (plusp power) (setq base (* base base))))))

;;; The largest integer whose square is at most N: Newton's method on
;;; integers, from a first guess that is too large.
(defun isqrt (n)
  (if (minusp n) (error "ISQRT of a negative number" n))
  (if (zerop n)
      0
      (let ((x (ash 1 (ceiling (integer-length n) 2))))
        (do ((y (floor (+ x (floor n x)) 2) (floor (+ x (floor n x)) 2)))
            ((>= y x) x)
          (setq x y)))))

;;; ------------------------------------------------------------------------
;;; Bits
;;;
;;; CL defines the bit operations on *two's complement*: -1 is all ones,
;;; going on forever to the left. Stored as sign and magnitude, a number is
;;; converted to two's complement in enough limbs, the operation is done
;;; limb by limb, and the result converted back — as CPython does.

(defun ash (integer count)
  (if (>= count 0)
      (* integer (expt 2 count))
      ;; `floor`, not `truncate`: shifting right rounds towards minus
      ;; infinity, as an arithmetic shift does.
      (nth-value 0 (floor integer (expt 2 (- count))))))

(defun integer-length (integer)
  (if (minusp integer)
      (integer-length (lognot integer))
      (let ((m (nth-value 1 (%magnitude integer))))
        (if (= (length m) 0)
            0
            (+ (* 30 (- (length m) 1)) (%bit-length (aref m (- (length m) 1))))))))

(defun %bit-length (limb)
  (let ((n 0))
    (do () ((= limb 0) n)
      (setq limb (%quotient limb 2))
      (setq n (+ n 1)))))

(defun lognot (integer) (- -1 integer))

;;; INTEGER in two's complement over LENGTH limbs.
(defun %twos (integer length)
  (multiple-value-bind (negative m) (%magnitude integer)
    (let ((out (make-array length :initial-element 0)))
      (if negative
          ;; -m is the complement of m - 1.
          (let ((m1 (%mag-sub m (vector 1))))
            (dotimes (i length out)
              (setf (aref out i)
                    (%fixnum-logxor (if (< i (length m1)) (aref m1 i) 0) %limb-mask))))
          (progn (dotimes (i (length m) out) (setf (aref out i) (aref m i))))))))

;;; The integer whose two's complement LIMBS are, the top bit giving the sign.
(defun %from-twos (limbs)
  (let ((top (aref limbs (- (length limbs) 1))))
    (if (< top 536870912)                       ; bit 29 clear: not negative
        (%make-integer nil (%mag-trim limbs))
        (let ((complement (make-array (length limbs))))
          (dotimes (i (length limbs))
            (setf (aref complement i) (%fixnum-logxor (aref limbs i) %limb-mask)))
          ;; -(complement + 1)
          (%make-integer t (%mag-add (%mag-trim complement) (vector 1)))))))

(defun %logop2 (op a b)
  (if (and (not (%bignump a)) (not (%bignump b)))
      (funcall op a b)
      (let ((length (+ 1 (max (length (nth-value 1 (%magnitude a)))
                              (length (nth-value 1 (%magnitude b)))))))
        (let ((x (%twos a length)) (y (%twos b length)) (out (make-array length)))
          (dotimes (i length)
            (setf (aref out i) (funcall op (aref x i) (aref y i))))
          (%from-twos out)))))

(defun logand (&rest integers)
  (let ((r -1)) (dolist (n integers r) (setq r (%logop2 #'%fixnum-logand r n)))))
(defun logior (&rest integers)
  (let ((r 0)) (dolist (n integers r) (setq r (%logop2 #'%fixnum-logior r n)))))
(defun logxor (&rest integers)
  (let ((r 0)) (dolist (n integers r) (setq r (%logop2 #'%fixnum-logxor r n)))))

(defun logbitp (index integer) (oddp (ash integer (- index))))

(defun logcount (integer)
  (let ((m (nth-value 1 (%magnitude (if (minusp integer) (lognot integer) integer))))
        (count 0))
    (dotimes (i (length m) count)
      (let ((limb (aref m i)))
        (do () ((= limb 0))
          (if (oddp limb) (setq count (+ count 1)))
          (setq limb (%quotient limb 2)))))))
