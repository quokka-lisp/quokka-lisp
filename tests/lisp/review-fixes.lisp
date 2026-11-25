;;;; What the November 2026 review found wrong in the library and the
;;;; compiler (#106 to #109), each as SBCL answers it. Tier 1: compiled only.

;;; `handler-case` with `:no-error` (#106): the clause receives the form's
;;; values when nothing is signalled, and is passed over when something is.
(handler-case 5 (error () 1) (:no-error (x) (list x)))
(handler-case (values 1 2) (error () 0) (:no-error (a b) (+ a b)))
(handler-case (values) (error () 0) (:no-error () :none))
(handler-case (error "x") (error () :signalled) (:no-error (x) (list x)))
(handler-case (car 5) (:no-error (x) (list x)) (type-error () :type-error))
;; The clause's body is outside the handlers: an error in it is not caught.
(handler-case
    (handler-case 1 (error () :inner) (:no-error (x) (error "from the clause")))
  (error () :outer))
;; Without the clause, as ever.
(handler-case 5 (error () 1))
(multiple-value-list (handler-case (values 1 2 3) (error () 0)))

;;; A clause for several types, `(or a b)` (#106).
(handler-case (car 5) ((or type-error program-error) () :either))
(handler-case (error "x") ((or type-error simple-error) () :either))
(handler-case (error "x") ((or) () :never) (error () :fell-through))
(handler-case
    (handler-case (error "x") ((or type-error program-error) () :either))
  (error () :not-one-of-them))
(handler-case (car 5) ((or program-error error) (c) (if c :has-the-condition)))

;;; `expt` with a ratio power is a root, and `evenp` and `oddp` are of
;;; integers (#107).
(= (expt 9 1/2) 3)
(= (expt 4 -1/2) 1/2)
(= (expt 8.0d0 1/3) 2)
(< 1.41 (expt 2 1/2) 1.42)
(list (expt 2 10) (expt 2 -2) (expt 1/2 3) (expt 2 0) (expt 0 0))
(handler-case (evenp 1/2) (type-error () :type-error))
(handler-case (oddp 1.5) (type-error () :type-error))
(handler-case (evenp 'a) (type-error () :type-error))
(list (evenp 0) (evenp 7) (evenp -4) (oddp 7) (oddp -3) (oddp 0))
(list (evenp (expt 2 70)) (oddp (+ 1 (expt 2 70))) (evenp (- (expt 2 70))))

;;; Backquote (#108): vectors, `,.`, and `,,@` in a nested template.
(let ((x 2)) `#(1 ,x))
(let ((x (list 2 3))) `#(1 ,@x 4))
`#(a b)
(let ((x 1)) `(a #(b ,x) c))
(let ((x (list 2 3))) `(1 ,.x))
(let ((x (list 2 3))) `(1 ,.x 4))
(let ((x '(1 2))) (eval ``(a ,,@x)))
(let ((x '(1 2))) (eval ``(a ,,@x b)))
(let ((b 1) (c '(2 3))) `(a ,b ,@c (d ,b) . e))
(let ((x '(a b))) `(1 ,@x))
(handler-case (read-from-string "`,@(list 1 2)") (reader-error () :reader-error))

;;; A macro's body is in a block named after the macro (#109).
(defmacro rf-macro (x) (when x (return-from rf-macro 1)) 2)
(list (rf-macro t) (rf-macro nil))
(macrolet ((m (x) (when x (return-from m 1)) 2)) (list (m nil) (m t)))
(defmacro rf-declared (x)
  "Documentation, then a declaration, then the block."
  (declare (ignorable x))
  (if x (return-from rf-declared ''yes))
  ''no)
(list (rf-declared t) (rf-declared nil))
