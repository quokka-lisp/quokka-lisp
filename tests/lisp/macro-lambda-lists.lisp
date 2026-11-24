;;; Step AC (#76): an expander is a function of the form and the environment,
;;; so `&whole` is the form and `&environment` is bound before anything else;
;;; `&whole`, `&key` and `&aux` in nested patterns and in `destructuring-bind`;
;;; an empty `&key`; default forms that see only the parameters to their left;
;;; `values-list`'s checks. Tier 1: compiled and SBCL only.

;; &whole at the top of a macro is the whole form, head included.
(defmacro whole-1 (&whole w arg) `(list ',w ',arg))
(whole-1 x)
(defmacro whole-2 (&whole w foo &rest bar) `(list ',w ',foo ',bar))
(list (whole-2 x) (whole-2 x y) (whole-2 x y z))
;; A pattern after a macro's top-level &whole, `(&whole (head a b) c d)`,
;; works too, as CLHS 3.4.4.1 and ansi-test's MACROLET.36 ask, but SBCL
;; refuses it, so it is not compared here.
(let ((x nil))
  (macrolet ((m (&whole w arg) `(progn (setq x (quote ,w)) ,arg)))
    (list (m 1) x)))

;; In a nested pattern, &whole is the list that pattern matches.
(defmacro whole-3 (arg1 (&whole w arg2)) `(list ',w ',arg1 ',arg2))
(whole-3 x (y))
(let ((x nil))
  (macrolet ((m ((&whole w arg)) `(progn (setq x (quote ,w)) ,arg)))
    (list (m (1)) x)))

;; The expander itself, through macro-function and macroexpand-1.
(funcall (macro-function 'whole-1) '(whole-1 q) nil)
(macroexpand-1 '(whole-2 a b))

;; &environment is bound first, so a default form can use it.
(defmacro inner-thing () 1)
(macrolet ((m (&optional (x (macroexpand '(inner-thing) env)) &environment env) x)) (m))

;; destructuring-bind: &whole is the list, which may be dotted.
(multiple-value-list (destructuring-bind (&whole w (x y)) '((a b)) (values x y w)))
(multiple-value-list (destructuring-bind (&whole x y z) '(a b) (values x y z)))
(multiple-value-list (destructuring-bind (w (&whole x y z)) '(1 (a b)) (values w x y z)))
(destructuring-bind (&whole (a . b) c . d) '(1 . 2) (list a b c d))

;; &key and &aux, at the top and in a nested pattern.
(destructuring-bind (a &key b (c 3 c-given)) '(1 :b 2) (list a b c c-given))
(destructuring-bind (a &key ((:the-b b) 0)) '(1 :the-b 5) (list a b))
(destructuring-bind ((a &key b) &aux (z (list a b))) '((1 :b 2)) z)
(destructuring-bind (&key a) '(:a 1 :a 2) a)
(handler-case (destructuring-bind (&key a) '(:b 1) a) (program-error () :program-error))
(handler-case (destructuring-bind (&key a) '(:a) a) (program-error () :program-error))
(destructuring-bind (&key a) '(:b 1 :allow-other-keys t) a)
(destructuring-bind (&key a &allow-other-keys) '(:b 1 :a 2) a)
(defmacro keyed (&key (op 'list) (args nil)) `(,op ,@args))
(list (keyed) (keyed :op + :args (1 2)))

;; nil is the empty pattern, in &optional too.
(destructuring-bind (a &optional nil) '(1) a)
(handler-case (destructuring-bind (a nil) '(1 2) a) (program-error () :program-error))

;; An empty &key still takes keyword arguments, and :allow-other-keys.
((lambda (&key) 'good) :allow-other-keys nil)
((lambda (&key) 'good) :allow-other-keys t :x 1)
(handler-case ((lambda (&key) 'bad) :x 1) (program-error () :program-error))
(flet ((f (&key) :called)) (f))

;; A default form sees only the parameters to its left.
(let ((x 1)) ((lambda (&optional (x (1+ x))) x)))
(let ((b 10)) ((lambda (&optional (a b) (b (1+ a))) (list a b))))
(let ((b 1)) ((lambda (&key (a b) b) (list a b)) :b 'x))
(let ((r :outer)) ((lambda (&optional (a r) &rest r) (list a r)) 1 2 3))
(let ((r :outer)) ((lambda (&optional (a r) &rest r) (list a r))))
((lambda (a &optional (b a b-given) &key (c (list a b))) (list a b b-given c)) 1)
((lambda (a &optional (b a b-given) &key (c (list a b))) (list a b b-given c)) 1 2 :c 3)

;; values-list wants exactly one proper list.
(multiple-value-list (values-list '(1 2 3)))
(handler-case (values-list) (program-error () :program-error))
(handler-case (values-list nil nil) (program-error () :program-error))
(handler-case (values-list '(a b . c)) (type-error () :type-error))
