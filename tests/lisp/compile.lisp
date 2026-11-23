;;; compile, compiled-function-p, special-operator-p and compiler macros
;;; (step R, #65). Tier 1: compiled and SBCL only.

;; A lambda expression: three values, the function first.
(let ((results (multiple-value-list (compile nil '(lambda (x y) (cons y x))))))
  (list (length results) (funcall (first results) 'a 'b) (second results) (third results)))

;; Literal objects keep their identity through compilation.
(let ((x (list 'a 'b)))
  (eq x (funcall (compile nil `(lambda () ',x)))))
(let ((x (copy-seq "abc")))
  (funcall (compile nil `(lambda () (eq ,x ,x)))))

;; The null lexical environment: a variable outside is not seen inside.
(let ((outer 1))
  (declare (ignorable outer))
  (handler-case (funcall (compile nil '(lambda () outer)))
    (error () :unbound)))

;; Arguments are evaluated once, in order.
(let ((i 0) a b)
  (list (funcall (compile (progn (setq a (incf i)) nil)
                          (progn (setq b (incf i)) '(lambda () 'z))))
        i a b))

;; A name gets the function, and compile returns the name.
(progn (fmakunbound 'compile-test-fn)
       (list (compile 'compile-test-fn '(lambda (x) (* x 2)))
             (compile-test-fn 21)
             (if (fboundp 'compile-test-fn) t nil)))

;; A name already defined: its function, as it is.
(defun compile-test-defined (x) (+ x 1))
(let ((before (symbol-function 'compile-test-defined)))
  (multiple-value-bind (name warnings failure) (compile 'compile-test-defined)
    (list name warnings failure
          (eq before (symbol-function 'compile-test-defined))
          (compile-test-defined 1))))

;; A function as the definition.
(funcall (compile nil #'car) '(1 2))

;; A name that is a macro gets a macro function.
(defmacro compile-test-macro (x) `(list ,x))
(progn (compile 'compile-test-macro '(lambda (form env) (declare (ignore env)) `(quote ,(cdr form))))
       (if (macro-function 'compile-test-macro) t nil))
(eval '(compile-test-macro 1 2))

;; An undefined name is an error.
(handler-case (compile 'compile-test-never-defined) (error () :error))

;; Every function is compiled.
(list (compiled-function-p #'car)
      (compiled-function-p (lambda (x) x))
      (compiled-function-p (compile nil '(lambda () 1)))
      (compiled-function-p 'car)
      (compiled-function-p nil)
      (compiled-function-p 3))

;; The special operators, and some names that are not.
(mapcar #'special-operator-p
        '(block catch eval-when flet function go if labels let let* load-time-value
          locally macrolet multiple-value-call multiple-value-prog1 progn progv quote
          return-from setq symbol-macrolet tagbody the throw unwind-protect))
(mapcar #'special-operator-p '(car defun lambda when cond nil t))
(handler-case (special-operator-p 1) (type-error () :type-error))

;; Compiler macros: none until defined; defining one returns the name.
(compiler-macro-function 'compile-test-square)
(defun compile-test-square (x) (* x x))
(define-compiler-macro compile-test-square (&whole form x)
  (if (numberp x) (* x x) form))
(if (compiler-macro-function 'compile-test-square) t nil)

;; The expander is a function of the form and the environment.
(funcall (compiler-macro-function 'compile-test-square) '(compile-test-square 5) nil)
(let ((form '(compile-test-square y)))
  (eq form (funcall (compiler-macro-function 'compile-test-square) form nil)))
(funcall (compiler-macro-function 'compile-test-square)
         '(funcall #'compile-test-square 6) nil)

;; Whether or not the rewrite happens, the answer is the function's.
(list (compile-test-square 7) (funcall (compile nil '(lambda (n) (compile-test-square n))) 8))

;; Applied by the compiler: this one's rewrite differs from the function, which
;; no conforming program would do, and which is why SBCL and Quokka can be
;; seen to agree on applying it.
(defun compile-test-tag (x) (list :function x))
(define-compiler-macro compile-test-tag (&whole form x &environment env)
  (declare (ignore env))
  (if (keywordp x) `(list :rewritten ,x) form))
(funcall (compile nil '(lambda () (compile-test-tag :k))))
(funcall (compile nil '(lambda () (compile-test-tag 'not-keyword))))
(funcall (compile nil '(lambda () (funcall #'compile-test-tag :k))))
;; Not when a local function shadows the name.
(funcall (compile nil '(lambda () (flet ((compile-test-tag (x) (list :local x)))
                                    (compile-test-tag :k)))))

;; Removing it.
(progn (setf (compiler-macro-function 'compile-test-tag) nil)
       (list (compiler-macro-function 'compile-test-tag)
             (funcall (compile nil '(lambda () (compile-test-tag :k))))))
