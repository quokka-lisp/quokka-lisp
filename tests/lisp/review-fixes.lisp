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

;;; The library's fast paths (#112) answer as the general ones did.
(list (member 'c '(a b c d)) (member 'z '(a b)) (member 3 '(1 2 3 4)) (member #\b '(#\a #\b)))
(list (length (member 2.0d0 '(1.0d0 2.0d0 3.0d0))) (member (expt 2 70) (list 1 (expt 2 70) 3)) (member 1/2 '(1/3 1/2)))
(list (member '(a) '((a) (b))) (member '(a) '((a) (b)) :test #'equal) (member 2 '((1) (2)) :key #'car))
(handler-case (member 'z '(a b . c)) (type-error () :type-error))
(list (assoc 'b '((a . 1) (b . 2))) (assoc 'z '((a . 1))) (assoc 'b '((a . 1) nil (b . 2))))
(list (assoc 2 '((1 . a) (2 . b))) (assoc "b" '(("a" . 1) ("b" . 2)) :test #'string=))
(handler-case (assoc 'z '((a . 1) b)) (type-error () :type-error))
(list (find 'b '(a b c)) (find 'z '(a b c)) (find nil '(a nil)) (find 2 '(1 2 3) :start 2))
(list (position 'c '(a b c)) (position 'z '(a b)) (position 'a '(a b a) :from-end t) (position #\b "abc"))
(list (mapcar #'1+ '(1 2 3)) (mapcar #'+ '(1 2 3) '(10 20)) (mapcar #'identity nil))
(let ((seen nil)) (list (mapc (lambda (x) (push x seen)) '(1 2 3)) seen))
(handler-case (mapcar #'identity '(1 2 . 3)) (type-error () :type-error))
(list (some #'evenp '(1 2 3)) (some #'evenp '(1 3)) (some #'identity '(nil 7 8)) (some #'evenp #(1 2)))
(list (every #'evenp '(2 4)) (every #'evenp '(2 3)) (every #'evenp nil) (every #'< '(1 2) '(2 3)))
(let ((calls 0)) (some (lambda (x) (incf calls) (evenp x)) '(2 3 5 7)) calls)
(list (subseq "hello" 1 3) (subseq "hello" 2) (subseq #(1 2 3 4) 1 3) (subseq '(1 2 3) 1) (copy-seq "ab"))
(handler-case (subseq "abc" 2 9) (error () :out-of-range))
(let ((s (copy-seq "abc"))) (list (eq s (copy-seq s)) (equal s (copy-seq s))))
(do ((i 0 (+ i 1)) (j 10 (- j 1)) (acc nil (cons (list i j) acc))) ((= i 3) (reverse acc)))
(do ((a 1 b) (b 2 a) (n 0 (+ n 1))) ((= n 3) (list a b)))
(do ((i 0 (+ i 1))) ((= i 3) i))
(let ((k 'b)) (list (case k (a 1) (b 2) (t 0)) (case 2 ((1 2) :low) (3 :three)) (case #\x (#\x :x))))
(list (case 2.0d0 (2.0d0 :same) (t :other)) (case (expt 2 70) (1180591620717411303424 :big) (t :other)))
(list (name-char "Space") (name-char "NEWLINE") (name-char "nl") (name-char "nope") (name-char 'tab))
(list (string-upcase "abC d") (string-downcase "ABc" :start 1) (string-upcase 'sym))

;;; A cleanup is compiled once, so a macro in it is expanded once (#111).
(defvar *cleanup-expansions* 0)
(defmacro counted-cleanup () (incf *cleanup-expansions*) nil)
(progn (unwind-protect 1 (counted-cleanup)) *cleanup-expansions*)
(let ((ran nil)) (catch 'x (unwind-protect (throw 'x 1) (push :a ran) (push :b ran))) ran)
(let ((ran nil))
  (list (multiple-value-list (unwind-protect (values 1 2) (push :ran ran))) ran))
(let ((ran nil))
  (block b (unwind-protect (return-from b 1) (unwind-protect (push 1 ran) (push 2 ran))))
  ran)
(block out (unwind-protect 1 (return-from out :from-the-cleanup)))
(let ((x 0)) (unwind-protect (setq x 1) (setq x 2)) x)
