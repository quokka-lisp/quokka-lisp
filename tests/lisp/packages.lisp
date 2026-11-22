;;;; Packages: finding them, symbols in them, exporting and using (#43).
;;;;
;;;; Tier 1, so this runs compiled and against SBCL, with SBCL's output as the
;;;; baseline. It never prints a symbol from a package other than the current
;;;; one, nor a package object: those need the Lisp printer (step G). Names and
;;;; statuses are compared instead.

;; The standard packages, by name and by nickname.
(package-name (find-package "COMMON-LISP"))
(package-name (find-package :cl))
(package-name (find-package "CL-USER"))
(package-name (find-package 'keyword))
(find-package "NO-SUCH-PACKAGE")
(package-nicknames :cl)
(package-name *package*)

;; Where symbols live.
(package-name (symbol-package 'car))
(package-name (symbol-package 'nil))
(package-name (symbol-package :foo))
(package-name (symbol-package 'a-symbol-of-our-own))
(symbol-package (make-symbol "LONELY"))
(symbol-name (make-symbol "LONELY"))

;; find-symbol and intern, with their second values.
(multiple-value-list (find-symbol "CAR" "CL"))
(multiple-value-list (find-symbol "CAR"))
(multiple-value-list (find-symbol "NOTHING-IS-CALLED-THIS"))
(multiple-value-list (intern "CAR"))
(eq (intern "CAR") 'car)
(eq (intern "A-SYMBOL-OF-OUR-OWN") 'a-symbol-of-our-own)
(multiple-value-list (find-symbol "LOOP" "COMMON-LISP"))
(multiple-value-list (find-symbol "FOO" "KEYWORD"))
(eq (intern "PLAIN-KEY" "KEYWORD") :plain-key)

;; A package of our own.
(package-name (defpackage :geometry (:use :cl) (:export :area :perimeter)))
(multiple-value-bind (s status) (find-symbol "AREA" :geometry) (list (symbol-name s) status))
(multiple-value-bind (s status) (find-symbol "CAR" :geometry) (list (symbol-name s) status))
(multiple-value-bind (s status) (intern "HELPER" :geometry) (list (symbol-name s) status))
(multiple-value-bind (s status) (intern "HELPER" :geometry) (list (symbol-name s) status))
(package-name (symbol-package (find-symbol "AREA" :geometry)))
(eq (find-symbol "AREA" :geometry) 'area)
(length (package-use-list :geometry))
(package-name (car (package-use-list :geometry)))

;; make-package, nicknames, export, use-package.
(package-name (make-package "SCRATCH" :nicknames '("SCR")))
(package-name (find-package "SCR"))
(multiple-value-bind (s status) (intern "PUBLIC" "SCRATCH") (list (symbol-name s) status))
(export (find-symbol "PUBLIC" "SCRATCH") "SCRATCH")
(multiple-value-bind (s status) (find-symbol "PUBLIC" "SCRATCH") (list (symbol-name s) status))
(use-package "SCRATCH" "GEOMETRY")
(multiple-value-bind (s status) (find-symbol "PUBLIC" "GEOMETRY") (list (symbol-name s) status))
(eq (find-symbol "PUBLIC" "GEOMETRY") (find-symbol "PUBLIC" "SCRATCH"))

;; in-package changes *package*, and changes back.
(progn (in-package :geometry) (package-name *package*))
(progn (in-package :cl-user) (package-name *package*))

;; packagep, and designators that are already packages.
(packagep (find-package :cl))
(packagep 'car)
(eq (find-package (find-package :cl)) (find-package :cl))

;; Errors: a package that exists already, and one that does not.
(make-package "SCRATCH")
(find-symbol "CAR" "NO-SUCH-PACKAGE")
