;;; The rest of the package system: conflicts, shadowing, importing,
;;; uninterning, deleting, renaming, iterating (step S, #66). Tier 1:
;;; compiled and SBCL only. No package object is printed: names are compared.

(defun names (symbols) (sort (mapcar #'symbol-name symbols) #'string<))
(defun pnames (packages) (sort (mapcar #'package-name packages) #'string<))
(defun status (name package) (nth-value 1 (find-symbol name package)))

(progn (defpackage "PS-A" (:use) (:export "ONE" "TWO" "SHARED")) t)
(progn (defpackage "PS-B" (:use) (:export "SHARED" "THREE")) t)
(progn (defpackage "PS-C" (:use "PS-A")) t)

;; Who uses whom, both ways.
(pnames (package-use-list "PS-C"))
(pnames (package-used-by-list "PS-A"))
(package-used-by-list "PS-B")

;; Using PS-B too would make two SHAREDs accessible in PS-C: refused, and
;; nothing changed.
(handler-case (use-package "PS-B" "PS-C") (package-error () :conflict))
(pnames (package-use-list "PS-C"))
(pnames (package-used-by-list "PS-B"))

;; Shadowing resolves it in advance.
(shadow "SHARED" "PS-C")
(names (package-shadowing-symbols "PS-C"))
(status "SHARED" "PS-C")
(use-package "PS-B" "PS-C")
(list (status "SHARED" "PS-C") (status "THREE" "PS-C") (status "ONE" "PS-C"))
(eq (find-symbol "SHARED" "PS-C") (find-symbol "SHARED" "PS-A"))

;; unuse-package undoes both directions.
(unuse-package "PS-B" "PS-C")
(list (pnames (package-use-list "PS-C")) (package-used-by-list "PS-B") (status "THREE" "PS-C"))

;; import: present, internal; a different accessible symbol of the name is
;; a conflict; importing a symbol with no home gives it one.
(progn (defpackage "PS-D" (:use)) t)
(import (find-symbol "ONE" "PS-A") "PS-D")
(list (status "ONE" "PS-D") (eq (find-symbol "ONE" "PS-D") (find-symbol "ONE" "PS-A")))
(handler-case (import (intern "ONE" "PS-C") "PS-D") (package-error () :conflict))
(let ((loner (make-symbol "LONER")))
  (import loner "PS-D")
  (list (package-name (symbol-package loner)) (status "LONER" "PS-D")))

;; shadowing-import displaces what is present.
(let ((mine (intern "TWO" "PS-D")))
  (shadowing-import (find-symbol "TWO" "PS-A") "PS-D")
  (list (eq (find-symbol "TWO" "PS-D") (find-symbol "TWO" "PS-A"))
        (symbol-package mine)
        (names (package-shadowing-symbols "PS-D"))))

;; export and unexport; exporting needs the symbol accessible, and checks
;; every user of the package.
(progn (intern "LATER" "PS-A") (export (find-symbol "LATER" "PS-A") "PS-A"))
(list (status "LATER" "PS-A") (status "LATER" "PS-C"))
(unexport (find-symbol "LATER" "PS-A") "PS-A")
(list (status "LATER" "PS-A") (status "LATER" "PS-C"))
(handler-case (export (make-symbol "NOWHERE") "PS-A") (package-error () :not-accessible))
(progn (intern "CLASH" "PS-C") (intern "CLASH" "PS-A") t)
(handler-case (export (find-symbol "CLASH" "PS-A") "PS-A") (package-error () :conflict))

;; unintern: present symbols only; the symbol loses its home.
(let ((s (intern "GOES" "PS-D")))
  (list (unintern s "PS-D") (symbol-package s) (status "GOES" "PS-D") (unintern s "PS-D")))
(unintern (find-symbol "ONE" "PS-A") "PS-C")

;; Iteration: accessible, external, and the iterator's four values.
(let ((seen nil)) (do-external-symbols (s "PS-A") (push s seen)) (names seen))
(let ((seen nil)) (do-symbols (s "PS-C") (push s seen)) (names seen))
(do-symbols (s "PS-B" :done) (declare (ignorable s)))
(let ((seen nil))
  (with-package-iterator (next "PS-B" :external)
    (do () (nil)
      (multiple-value-bind (more sym how pkg) (next)
        (if (not more) (return))
        (push (list (symbol-name sym) how (package-name pkg)) seen))))
  (sort seen #'string< :key #'car))
(let ((n 0))
  (with-package-iterator (next '("PS-C" "PS-D") :internal :inherited)
    (do () ((not (next))) (incf n)))
  n)
(let ((n 0)) (do-all-symbols (s) (if (eq s 'car) (incf n))) (> n 0))
(names (find-all-symbols "SHARED"))

;; A declaration at the start of the body covers the result form too: the
;; loop macros used to leave it in their tagbody, where it did nothing.
(let ((x :good))
  (declare (special x))
  (let ((x :bad))
    (declare (ignorable x))
    (list (do-symbols (s "PS-C" x) (declare (special x) (ignorable s)))
          (dolist (e '(1) x) (declare (special x) (ignorable e)))
          (dotimes (i 1 x) (declare (special x) (ignorable i))))))

;; defpackage's options, in CLHS's order whatever order they are written in.
(progn (defpackage "PS-E" (:export "SHARED") (:use "PS-A" "PS-B")
         (:shadowing-import-from "PS-B" "SHARED") (:import-from "PS-A" "TWO")
         (:documentation "A test.") (:size 10))
       t)
(list (eq (find-symbol "SHARED" "PS-E") (find-symbol "SHARED" "PS-B"))
      (status "SHARED" "PS-E") (status "TWO" "PS-E") (status "THREE" "PS-E"))

;; Renaming; a name that is taken is refused.
(package-name (rename-package "PS-D" "PS-DD" '("PS-D-NICK")))
(list (find-package "PS-D") (package-name (find-package "PS-D-NICK")))
(handler-case (rename-package "PS-DD" "PS-A") (package-error () :taken))

;; Deleting: refused while used; afterwards the object has no name, its
;; symbols no home, and deleting again answers nil.
(handler-case (delete-package "PS-A") (package-error () :used))
(let ((three (find-symbol "THREE" "PS-B")))
  (delete-package "PS-E")
  (let ((ps-b (find-package "PS-B")))
    (list (delete-package ps-b) (package-name ps-b) (find-package "PS-B")
          (symbol-package three) (delete-package ps-b))))
(handler-case (delete-package "PS-NEVER-MADE") (package-error () :no-such-package))

;; gentemp makes fresh symbols in the current package.
(let ((s (gentemp "PS-TEMP")))
  (list (eq (symbol-package s) *package*) (not (eq s (gentemp "PS-TEMP")))))
