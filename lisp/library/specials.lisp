;;;; Special variables: `defvar` and `defparameter`.
;;;;
;;;; The first file of the tier-1 library (#39). Not part of the prelude: the
;;;; prelude is interpreted at boot and must stay tier 0 (rule 5), and special
;;;; variables are tier 1. This file is loaded into the *compiled* world, by
;;;; the image's own compiler, after `boot::build_image` has built it — so it
;;;; is compiled only, and the interpreter never learns what is in it.
;;;;
;;;; What makes a variable special is a mark the compiler keeps
;;;; (`%proclaim-special`, in `lisp/compiler/compile.lisp`): every binding of a
;;;; marked symbol — `let`, `let*`, a lambda parameter — is compiled as a
;;;; dynamic binding, `DBIND`, rather than a lexical one. Reading and assigning
;;;; it need nothing new; they were already `GREF` and `GSET`, the symbol's
;;;; value cell, because with shallow binding (#40) that is where the current
;;;; value lives.

;;; `(defvar name [value [documentation]])`
;;;
;;; Proclaims NAME special and, if it has no value yet, gives it VALUE — so
;;; that loading a file twice does not reset a variable the program has been
;;; using. Returns NAME.
;;;
;;; The proclamation is made twice on purpose. Once here, while the macro is
;;; being *expanded* — which is during compilation, so any binding of NAME
;;; compiled after this point in the same top-level form is already special,
;;; which is what Common Lisp's compile-time processing of `defvar` is for.
;;; And once in the expansion, for when the code runs.
(defmacro defvar (name &optional (value nil value-p) documentation)
  (%proclaim-special name)
  (if value-p
      `(progn (%proclaim-special ',name)
              (if (boundp ',name) nil (setq ,name ,value))
              ',name)
      `(progn (%proclaim-special ',name) ',name)))

;;; `(defparameter name value [documentation])`
;;;
;;; The same, except that VALUE is always assigned: a parameter is something
;;; the file means to set, every time it is loaded.
(defmacro defparameter (name value &optional documentation)
  (%proclaim-special name)
  `(progn (%proclaim-special ',name)
          (setq ,name ,value)
          ',name))
