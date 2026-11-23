;;; Step U (#68): the small functions ansi-test asked for. Tier 1: compiled
;;; and SBCL only.

;; parse-integer: whitespace, a sign, a radix, a range, junk.
(multiple-value-list (parse-integer "123"))
(multiple-value-list (parse-integer "  -42  "))
(multiple-value-list (parse-integer "+7"))
(multiple-value-list (parse-integer "ff" :radix 16))
(multiple-value-list (parse-integer "z" :radix 36))
(multiple-value-list (parse-integer "xx123yy" :start 2 :end 5))
(multiple-value-list (parse-integer "12abc" :junk-allowed t))
(multiple-value-list (parse-integer "  abc" :junk-allowed t))
(multiple-value-list (parse-integer "" :junk-allowed t))
(multiple-value-list (parse-integer "123456789012345678901234567890"))
(handler-case (parse-integer "12abc") (parse-error () :parse-error))
(handler-case (parse-integer "   ") (parse-error () :parse-error))
(handler-case (parse-integer "-") (parse-error () :parse-error))

;; The four-letter c...r, read and set.
(let ((tree '((((1 2) 3) 4) ((5 6) 7) 8)))
  (list (caaaar tree) (cdaaar tree) (cadaar tree) (cddaar tree)))
(let ((tree '((a (b c) d))))
  (list (caadar tree) (cdadar tree) (caddar tree) (cdddar tree)))
(let ((x (list 1 2 3 4 5)))
  (list (cadddr x) (cddddr x) (cdddr x)))
(let ((x (copy-tree '((((a)))))))
  (setf (caaaar x) 'z)
  x)
(let ((x (list 1 2 3 4 5)))
  (setf (cadddr x) 'four (cddddr x) '(end))
  x)
(let ((x (copy-tree '((a b) (c d)))))
  (setf (caadr x) 'cc (cdadr x) '(dd) (cddar x) '(bb))
  x)

;; nset-exclusive-or and nsubst-if, as their non-destructive twins.
(sort (nset-exclusive-or (list 1 2 3 4) (list 3 4 5 6)) #'<)
(nsubst-if 'x #'numberp (list 1 'a (list 2 'b)))
(nsubst-if-not 'x #'symbolp (list 1 'a (list 2 'b)))

;; prog and prog*: bindings, go, return.
(prog ((i 0) (acc nil))
 top
   (if (= i 3) (return (nreverse acc)))
   (push i acc)
   (setq i (+ i 1))
   (go top))
(prog* ((a 1) (b (+ a 1))) (return (list a b)))
(prog () (return))
(prog ((x 1)) (declare (ignorable x)))

;; multiple-value-setq: assigns, and yields the first value.
(let (a b c)
  (list (multiple-value-setq (a b c) (values 1 2)) a b c))
(let (q r)
  (multiple-value-setq (q r) (truncate 17 5))
  (list q r))

;; Characters.
(list (char-int #\A) (char-int #\a))
(mapcar #'standard-char-p (list #\a #\Space #\Newline #\~ (code-char 7) (code-char 200)))

;; copy-symbol.
(let ((s (make-symbol "ORIGINAL")))
  (setf (symbol-value s) 42 (get s 'colour) 'red)
  (let ((plain (copy-symbol s)) (full (copy-symbol s t)))
    (list (symbol-name plain) (eq plain s) (symbol-package plain) (boundp plain)
          (symbol-plist plain) (symbol-value full) (get full 'colour))))

;; function-lambda-expression: the expression may always be unknown.
(nth-value 1 (function-lambda-expression (lambda (x) x)))

;; print-unreadable-object, with its body only (the type name and identity
;; are each implementation's own).
(string= (with-output-to-string (s)
           (print-unreadable-object (42 s) (write-string "forty-two" s)))
         (concatenate 'string "#" "<forty-two>"))
(length (with-output-to-string (s) (print-unreadable-object ('sym s))))
(handler-case
    (let ((*print-readably* t))
      (with-output-to-string (s) (print-unreadable-object (1 s) (write-string "x" s))))
  (print-not-readable () :not-readable))

;; copy-structure.
(defstruct u-point x y)
(let* ((p (make-u-point :x 1 :y 2))
       (q (copy-structure p)))
  (setf (u-point-x q) 10)
  (list (u-point-x p) (u-point-x q) (u-point-y q) (eq p q)))

;; ccase is ecase until there are restarts; both signal a type-error.
(let ((k 2)) (ccase k (1 :one) ((2 3) :two-or-three)))
(let ((k 9)) (handler-case (ccase k (1 :one)) (type-error () :type-error)))
(handler-case (ecase 9 (1 :one)) (type-error () :type-error))
(let ((x 'b)) (ccase x (a 1) (b 2)))

;; The inverse hyperbolic functions. `asinh` and `acosh` digit for digit
;; with SBCL's; `atanh` to within a unit in the last place, because the C
;; libraries SBCL calls round it differently on different systems (macOS
;; answers 0.5493061443340549 for (atanh 0.5d0), one ulp above the nearest
;; double to ½ ln 3, which `libm` gives). Doubles print without an exponent
;; marker only when they are the default format; an integer argument is left
;; out, since SBCL answers a single-float for it.
(setq *read-default-float-format* 'double-float)
(list (asinh 0.5d0) (asinh -2.0d0) (acosh 1.0d0) (acosh 10.0d0) (asinh 1d-10))
(list (< (abs (- (atanh 0.5d0) 0.5493061443340548d0)) 2d-16)
      (< (abs (- (atanh -0.25d0) -0.2554128118829953d0)) 2d-16)
      (atanh 0.0d0)
      (sinh (asinh 3.0d0)))
(setq *read-default-float-format* 'single-float)

;; define-symbol-macro: a global symbol macro, read and set through, hidden
;; by a lexical binding, and seen by macroexpand-1.
(defvar *u-cell* (list 1 2))
(define-symbol-macro u-head (car *u-cell*))
(list u-head (let ((u-head 'local)) u-head))
(progn (setq u-head 'changed) *u-cell*)
(progn (setf u-head 'again) (list u-head *u-cell*))
(multiple-value-list (macroexpand-1 'u-head))
(funcall (lambda () u-head))

;; Found by ansi-test once these functions existed: the test of
;; set-exclusive-or always gets list-1's element first; multiple-value-setq
;; of no variables yields the form's first value; a symbol-macrolet body may
;; declare a variable special.
(sort (set-exclusive-or (list 1 2 3 4) (list 3 4 5 6) :test (lambda (x y) (= x (1- y)))) #'<)
(list (multiple-value-setq () :good) (multiple-value-setq () (values 'a 'b)))
(let ((x :good))
  (declare (special x))
  (let ((x :bad))
    (declare (ignorable x))
    (symbol-macrolet () (declare (special x)) x)))
(handler-case (eval '(symbol-macrolet ((x 10)) (declare (special x)) 20))
  (program-error () :program-error))
