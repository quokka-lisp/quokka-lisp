;;;; Integer division (#46).
;;;;
;;;; Quokka had `+`, `-`, `*` and the comparisons, and no way to divide —
;;;; nothing needed one until the printer had to turn a number into digits in
;;;; a base. `%quotient` and `rem` are primitives, because Rust's `/` and `%`
;;;; already truncate towards zero exactly as Common Lisp's `truncate` does;
;;;; everything else here is Lisp over them. Fixnums only, until bignums
;;;; (step L) redo arithmetic as a whole.

;;; `(truncate n d)` — the quotient rounded towards zero, and the remainder.
(defun truncate (number &optional (divisor 1))
  (values (%quotient number divisor) (rem number divisor)))

;;; `(floor n d)` — the quotient rounded towards negative infinity. It differs
;;; from `truncate` only when the remainder is not zero and its sign is not
;;; the divisor's: -7/2 truncates to -3 rem -1, and floors to -4 rem 1.
(defun floor (number &optional (divisor 1))
  (let ((quotient (%quotient number divisor))
        (remainder (rem number divisor)))
    (if (and (not (eq remainder 0)) (not (eq (< remainder 0) (< divisor 0))))
        (values (- quotient 1) (+ remainder divisor))
        (values quotient remainder))))

;;; `(mod n d)` — `floor`'s remainder, which has the divisor's sign.
(defun mod (number divisor)
  (nth-value 1 (floor number divisor)))

(defun abs (number) (if (< number 0) (- number) number))
(defun zerop (number) (eq number 0))
(defun plusp (number) (> number 0))
(defun minusp (number) (< number 0))
