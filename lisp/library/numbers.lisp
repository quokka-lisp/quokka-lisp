;;;; Integer division (#46).
;;;;
;;;; Quokka had `+`, `-`, `*` and the comparisons, and no way to divide —
;;;; nothing needed one until the printer had to turn a number into digits in
;;;; a base. `%quotient` and `rem` are primitives, because Rust's `/` and `%`
;;;; already truncate towards zero exactly as Common Lisp's `truncate` does;
;;;; everything else here is Lisp over them. Fixnums only, until bignums
;;;; (step L) redo arithmetic as a whole.

;;; `(truncate n d)` — the quotient rounded towards zero, and the remainder.
;;; A float, either side, is `floats.lisp`'s (step M, #17).
(defun truncate (number &optional (divisor 1))
  (if (or (floatp number) (floatp divisor))
      (%truncate-float number divisor)
      (values (%quotient number divisor) (rem number divisor))))

;;; `(floor n d)` — the quotient rounded towards negative infinity. It differs
;;; from `truncate` only when the remainder is not zero and its sign is not
;;; the divisor's: -7/2 truncates to -3 rem -1, and floors to -4 rem 1.
(defun floor (number &optional (divisor 1))
  (multiple-value-bind (quotient remainder) (truncate number divisor)
    (if (and (not (zerop remainder)) (not (eq (< remainder 0) (< divisor 0))))
        (values (- quotient 1) (+ remainder divisor))
        (values quotient remainder))))

;;; `(mod n d)` — `floor`'s remainder, which has the divisor's sign.
(defun mod (number divisor)
  (nth-value 1 (floor number divisor)))

;;; A float by its sign bit, so that (abs -0.0) is 0.0, as in SBCL.
(defun abs (number)
  (if (floatp number)
      (if (%negative-float-p number) (- number) number)
      (if (< number 0) (- number) number)))
;;; `=`, not `eq`, since 0.0 and -0.0 are zero too (step M).
(defun zerop (number) (= number 0))
(defun plusp (number) (> number 0))
(defun minusp (number) (< number 0))

;;; The comparisons the Rust table does not have (#54, J1). `<`, `>` and `=`
;;; are primitives; these are Lisp over them, and take any number of
;;; arguments, as CL's do: true when every adjacent pair is in order.
(defun %pairwise (test number more)
  (cond ((null more) t)
        ((funcall test number (car more)) (%pairwise test (car more) (cdr more)))
        (t nil)))

;;; Two arguments, the usual case, go straight to the primitive: the compiler
;;; and the library's loops call these constantly, and the general walk
;;; costs a closure and a call per pair (#79).
(defun <= (number &rest more)
  (if (and more (null (cdr more)))
      (not (> number (car more)))
      (%pairwise (lambda (a b) (not (> a b))) number more)))
(defun >= (number &rest more)
  (if (and more (null (cdr more)))
      (not (< number (car more)))
      (%pairwise (lambda (a b) (not (< a b))) number more)))

;;; `/=` is every pair, not only adjacent ones: (/= 1 2 1) is false.
(defun /= (number &rest more)
  (cond ((null more) t)
        ((%none-equal number more) (apply #'/= more))
        (t nil)))

(defun %none-equal (number others)
  (cond ((null others) t)
        ((= number (car others)) nil)
        (t (%none-equal number (cdr others)))))

(defun 1+ (number) (+ number 1))
(defun 1- (number) (- number 1))
(defun evenp (integer) (zerop (mod integer 2)))
(defun oddp (integer) (not (evenp integer)))

(defun max (number &rest more)
  (if (null more)
      number
      (apply #'max (if (> (car more) number) (car more) number) (cdr more))))

(defun min (number &rest more)
  (if (null more)
      number
      (apply #'min (if (< (car more) number) (car more) number) (cdr more))))
