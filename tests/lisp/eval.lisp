;;;; eval and load (#47).
;;;;
;;;; Tier 1, so this runs compiled and against SBCL. Since #47 the compiled
;;;; leg reads every form with the Lisp reader and prints with the Lisp
;;;; printer, so `in-package` here changes how the next form is read, as it
;;;; does in SBCL. `load` of a file is tested in `differential.rs`, through a
;;;; host that has the file; here it loads from streams.

;; eval compiles its form and runs it.
(eval '(+ 1 2))
(eval (list 'car ''(a b)))
(eval 42)
(eval "self-evaluating")
(eval :keyword)
(eval '(let ((x 1)) (+ x 1)))
(eval (read-from-string "(list 1 2 3)"))
(multiple-value-list (eval '(values 1 2)))
(funcall (eval '(lambda (x) (+ x 1))) 1)
(progn (eval '(defun twice (x) (* 2 x))) (twice 21))
(eval '(defvar *counter* 0))
(progn (eval '(setq *counter* (+ *counter* 1))) *counter*)

;; eval sees the dynamic environment, never the lexical one.
(defvar *dynamic* 'outer)
(let ((*dynamic* 'inner)) (eval '*dynamic*))
(let ((lexical 1)) (eval 'lexical))

;; Errors, at run time and at compile time.
(eval '(car 1))
(eval '(if))

;; load from a stream: every form, in order, and t.
(with-input-from-string (s "(defun from-stream () :loaded) (defvar *loaded* 7)") (load s))
(from-stream)
*loaded*
(with-input-from-string (s "") (load s))
(with-input-from-string (s "(car 1)") (load s))

;; load rebinds *package*, so an in-package inside stays inside.
(package-name (defpackage :eval-test (:use :cl)))
(progn (with-input-from-string (s "(in-package :eval-test)") (load s)) (package-name *package*))

;; At top level it does not: the next form is read in the new package.
(package-name (in-package :eval-test))
(package-name (symbol-package 'fresh-here))
(defun whose () :eval-test)
(package-name (in-package :cl-user))
(package-name *package*)
(package-name (symbol-package 'eval-test::fresh-here))
(eval-test::whose)
(eq 'eval-test::fresh-here (find-symbol "FRESH-HERE" :eval-test))
(find-symbol "WHOSE" :cl-user)
