;;; What the compiler and the prelude skipped (step AG, #80): declarations
;;; in multiple-value-bind, flet, labels and macrolet bodies; special
;;; parameters seen by the defaults to their right; integer go tags;
;;; nested patterns in a macro's &optional and &key; a local function that
;;; hides a local macro; notinline and compiler macros; throw to no catch;
;;; lambda and defmacro as macros; incf's order. Tier 1: compiled and SBCL.

;; multiple-value-bind: an empty body is nil; declarations reach the
;; variables.
(multiple-value-bind () :foo)
(multiple-value-bind () (values 1 2 3))
(multiple-value-bind (a) (values 1 2) (declare (ignore a)))
(let ((z 0) x y)
  (declare (special z))
  (flet ((%x () (symbol-value 'x))
         (%y () (symbol-value 'y))
         (%z () (symbol-value 'z)))
    (multiple-value-bind (x y z) (values 1 2 3)
      (declare (special x y))
      (list (%x) (%y) (%z)))))

;; A special parameter is bound before the defaults to its right run.
(let ((x :bad))
  (declare (special x))
  (flet ((%f () x))
    (list ((lambda (x &aux (y (%f))) (declare (special x)) y) :good)
          ((lambda (x &optional (y (%f))) (declare (special x)) y) :good))))

;; Declarations in flet, labels and macrolet bodies.
(let ((x :good))
  (declare (special x))
  (let ((x :bad))
    (list (macrolet () (declare (special x)) x)
          (flet ((f () 1)) (declare (special x)) x))))

;; Integer go tags are compared with eql.
(block done
  (tagbody (go around) 10 (return-from done 'ten) around (go 10)))

;; Nested patterns in a macro lambda list's &optional and &key.
(macrolet ((%m (x &optional ((y z) '(2 3))) `(quote (,x ,y ,z))))
  (list (%m a) (%m a (b c))))
(macrolet ((%m (&key ((:a (b c)) '(3 4))) `(quote (,c ,b))))
  (list (%m :a (1 2)) (%m)))
(defmacro opt-dotted (&optional ((x . y) '(a . b))) `(list ',x ',y))
(list (opt-dotted) (opt-dotted (c d)))

;; A local function hides a local macro of the same name.
(macrolet ((%f () :bad)) (flet ((%f () :good)) (%f)))
(macrolet ((%f () :bad)) (labels ((%f () :good)) (%f)))

;; A compiler macro is not applied where its function is notinline.
(defun cm-target (x y) (list x y))
(defvar *cm-applied* nil)
(define-compiler-macro cm-target (x y) (setq *cm-applied* t) `(list ,x ,y))
(list (funcall (compile nil '(lambda (a b) (declare (notinline cm-target)) (cm-target a b))) 5 11)
      *cm-applied*)

;; A throw to a tag nobody catches is a control-error, and catchable.
(handler-case (throw (gensym) nil) (control-error () :control-error))

;; lambda and defmacro are macros.
(not (null (macro-function 'lambda)))
(not (null (macro-function 'defmacro)))
(funcall (eval (macroexpand '(lambda () 10))))
(special-operator-p 'lambda)
(handler-case (funcall (macro-function 'lambda)) (program-error () :program-error))

;; incf and decf read the place after evaluating the delta (CLHS 5.1.3).
(let ((x 0)) (list (incf x (setf x 1)) x))
(let ((x 0)) (list (decf x (setf x 1)) x))
(let ((v (vector 1 2)) (i 0)) (incf (aref v (incf i)) 10) (list v i))
