;;; Step Y (#72): `#n=` and `#n#`, `gensym`'s argument and counter, the
;;; standard limit constants, and the block around a long `defsetf`'s body.
;;; Tier 1: compiled and SBCL only. The limits differ from SBCL's (its are
;;; 2^62 - 1), so only what CLHS promises of them is printed.

;; A label used after its object is finished: the same object, not a copy.
(let ((x (read-from-string "(#1=(a b) #1# #1#)")))
  (list x (eq (first x) (second x)) (eq (second x) (third x))))

;; A label inside a lambda list, as ansi-test writes one.
(eval (read-from-string "((lambda (#1=#:foo) #1#) 17)"))
(eval (read-from-string "((lambda (&optional #1=#:foo) #1#))"))

;; A label used inside its own object: circular structure.
(let ((x (read-from-string "#1=(a b . #1#)")))
  (list (car x) (cadr x) (eq x (cddr x))))
(let ((x (read-from-string "#1=(#1# . #1#)")))
  (and (eq x (car x)) (eq x (cdr x))))
(let ((x (read-from-string "#1=#(1 #1# 3)")))
  (list (aref x 0) (eq x (aref x 1)) (aref x 2)))
(let ((x (read-from-string "#2A((1 #1=(a)) (#1# 4))")))
  (eq (aref x 0 1) (aref x 1 0)))

;; Labels nest, and an inner object may refer to an outer one.
(let ((x (read-from-string "#1=(a #2=(b . #1#) #2#)")))
  (list (eq (second x) (third x)) (eq x (cdr (second x)))))

;; What *print-circle* writes reads back to the same shape.
(let* ((*print-circle* t)
       (x (list 1 2 3)))
  (setf (cdr (last x)) x)
  (let ((y (read-from-string (prin1-to-string x))))
    (list (prin1-to-string y) (eq y (cdddr y)))))

;; Labels belong to one call to read: the same number is free again after.
(let ((in (make-string-input-stream "#1=(a) #1=(b)")))
  (list (read in) (read in)))

;; Errors: no label, a label defined twice, a label with no number.
(handler-case (read-from-string "#1#") (error () :no-such-label))
(handler-case (read-from-string "(#1=a #1=b)") (error () :defined-twice))
(handler-case (read-from-string "#=a") (error () :no-number))

;; Suppressed, labels read as nothing.
(let ((*read-suppress* t)) (read-from-string "(#1=(a) #1#)"))

;; gensym: a string prefix, an integer, the counter a program may bind.
(let ((*gensym-counter* 1327)) (symbol-name (gensym "FOO")))
(let ((*gensym-counter* 1)) (symbol-name (gensym)))
(let ((*gensym-counter* 12345)) (gensym) *gensym-counter*)
(let ((*gensym-counter* 10)) (list (symbol-name (gensym 123)) *gensym-counter*))
(let ((*gensym-counter* 99999999999999999999)) (list (symbol-name (gensym "X")) *gensym-counter*))
(symbol-package (gensym))
(handler-case (gensym t) (type-error () :type-error))

;; The limits: what CLHS promises of them.
(list (constantp 'call-arguments-limit) (>= call-arguments-limit 50)
      (constantp 'lambda-parameters-limit) (>= lambda-parameters-limit 50)
      (>= call-arguments-limit lambda-parameters-limit)
      (constantp 'multiple-values-limit) (>= multiple-values-limit 20))
(let ((args (make-list (min 200 (1- call-arguments-limit)) :initial-element 'a)))
  (length (apply #'list args)))
(constantp 'lambda-list-keywords)
(subsetp '(&allow-other-keys &aux &body &environment &key &optional &rest &whole)
         lambda-list-keywords)

;; Every float format has its limits, and they are floats of that format.
(mapcar (lambda (name) (and (constantp name) (floatp (symbol-value name))))
        '(most-positive-short-float least-positive-short-float
          least-positive-normalized-short-float most-negative-single-float
          least-negative-single-float most-positive-long-float
          least-positive-long-float short-float-epsilon
          single-float-negative-epsilon long-float-epsilon))
(list (plusp least-positive-single-float) (minusp least-negative-long-float)
      (<= most-positive-short-float most-positive-double-float))

;; A long defsetf's body is in a block named after the access function.
(defun first-of (x) (car x))
(defsetf first-of (place) (value)
  (return-from first-of `(setf (car ,place) ,value)))
(let ((x (cons 'a 'b)))
  (list (setf (first-of x) 'c) x))
