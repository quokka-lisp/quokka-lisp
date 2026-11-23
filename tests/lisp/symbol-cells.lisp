;;;; Symbols and their cells (#63, step P).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;; It asks nothing CLHS leaves open: which true value `remprop` and
;;;; `fboundp` return, or whether `constantp` folds `(+ 1 2)`.

;; Property lists.
(defvar *plist-sym* (make-symbol "P"))
(symbol-plist *plist-sym*)
(list (get *plist-sym* 'colour) (get *plist-sym* 'colour :none))
(progn (setf (get *plist-sym* 'colour) 'red) (get *plist-sym* 'colour))
(progn (setf (get *plist-sym* 'size) 3) (symbol-plist *plist-sym*))
(progn (setf (get *plist-sym* 'colour) 'blue) (symbol-plist *plist-sym*))
(list (not (null (remprop *plist-sym* 'colour))) (symbol-plist *plist-sym*))
(list (not (null (remprop *plist-sym* 'colour))) (symbol-plist *plist-sym*))
(progn (setf (symbol-plist *plist-sym*) (list 'a 1 'b 2)) (get *plist-sym* 'b))
(list (not (null (remprop *plist-sym* 'b))) (symbol-plist *plist-sym*))
(multiple-value-list (get-properties '(a 1 b 2 c 3) '(c b)))
(multiple-value-list (get-properties '(a 1) '(z)))
(let ((n 0)) (setf (get *plist-sym* 'k (incf n)) 'v) (list n (get *plist-sym* 'k)))

;; Function cells.
(defun cell-f (x) (* x 10))
(mapcar (lambda (name) (not (null (fboundp name))))
        (list 'cell-f 'car 'when 'if (make-symbol "NOWHERE")))
(list (functionp #'car) (functionp #'cell-f) (functionp (lambda (x) x))
      (functionp 'car) (functionp 12) (functionp nil))
(funcall (fdefinition 'cell-f) 4)
(progn (setf (fdefinition 'cell-g) (lambda (x) (+ x 1))) (cell-g 1))
(progn (fmakunbound 'cell-g) (fboundp 'cell-g))
(handler-case (cell-g 1) (undefined-function () :undefined))
(handler-case (fdefinition 'cell-g) (undefined-function () :undefined))

;; macro-function, with and without an environment.
(defmacro cell-twice (x) `(list ,x ,x))
(list (not (null (macro-function 'cell-twice))) (macro-function 'cell-f)
      (macro-function 'car))
(funcall (macro-function 'cell-twice) '(cell-twice 3) nil)
(defmacro cell-local-p (name &environment env)
  (if (macro-function name env) :local-or-global :none))
(list (cell-local-p cell-twice) (cell-local-p cell-f)
      (macrolet ((cell-inner () 1)) (cell-local-p cell-inner)))
(progn (setf (macro-function 'cell-swap)
             (lambda (form env) env (list 'list (third form) (second form))))
       t)
(cell-swap 1 2)

;; defconstant and constantp.
(defconstant +cell-limit+ 10)
+cell-limit+
(defconstant +cell-limit+ 10)
(handler-case (eval '(defconstant +cell-limit+ 11)) (error () :refused))
(list (constantp 1) (constantp "s") (constantp :k) (constantp t) (constantp nil)
      (constantp ''x) (constantp '+cell-limit+) (constantp 'cell-f)
      (constantp '(quote)))
(handler-case (eval '(let ((+cell-limit+ 1)) +cell-limit+)) (program-error () :refused))
(handler-case (eval '(let* ((t 1)) t)) (program-error () :refused))
(handler-case (eval '(setq +cell-limit+ 1)) (program-error () :refused))
(handler-case (eval '(setq :k 1)) (program-error () :refused))
(handler-case (eval '((lambda (nil) 1) 2)) (program-error () :refused))
(handler-case (eval '(symbol-macrolet ((+cell-limit+ 3)) +cell-limit+)) (program-error () :refused))
