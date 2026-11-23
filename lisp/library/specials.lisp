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
;;; `(proclaim '(special *x*))` and `(declaim (special *x*))` — the same as
;;; `defvar` without a value (#62, part 5). Every other kind of
;;; proclamation is advice, and ignored. `declaim` proclaims as it is
;;; expanded too, so that a binding compiled later in the same form is
;;; already dynamic, as `defvar` does.
(defun proclaim (specifier)
  ;; A declaration specifier is a symbol and a proper list of arguments;
  ;; anything else is an error even when the kind would be ignored — which
  ;; ansi-test checks, and the first measurement of this missed (#62).
  (if (not (and (consp specifier) (symbolp (car specifier))
                (%proper-list-p (cdr specifier))))
      (error "not a declaration specifier" specifier))
  (if (eq (car specifier) 'special)
      (%proclaim-all (cdr specifier)))
  nil)

(defun %proper-list-p (object)
  (cond ((null object) t)
        ((consp object) (%proper-list-p (cdr object)))
        (t nil)))

(defun %proclaim-all (names)
  (if (consp names)
      (progn (%proclaim-special (car names))
             (%proclaim-all (cdr names)))))

(defmacro declaim (&rest specifiers)
  (%proclaim-each specifiers)
  `(progn ,@(%quoted-proclaims specifiers) nil))

(defun %proclaim-each (specifiers)
  (if (consp specifiers)
      (progn (proclaim (car specifiers))
             (%proclaim-each (cdr specifiers)))))

(defun %quoted-proclaims (specifiers)
  (if (consp specifiers)
      (cons (list 'proclaim (list 'quote (car specifiers)))
            (%quoted-proclaims (cdr specifiers)))
      nil))
;;; `(defconstant name value [documentation])` (#63, part 4) — a variable
;;; whose value never changes. Proclaimed constant as it is expanded, so the
;;; compiler refuses a binding or `setq` of it in the same form and after;
;;; marked on the symbol's property list, which `constantp` reads; and given
;;; VALUE unless it already has one `eql` to it, so loading the same file
;;; twice is harmless, as in SBCL. A different value is an error.
(defmacro defconstant (name value &optional documentation)
  (%proclaim-constant name)
  `(progn (%proclaim-constant ',name)
          (%define-constant ',name ,value)
          ',name))

(defun %define-constant (name value)
  (cond ((not (boundp name)))
        ((eql (symbol-value name) value))
        (t (error "defconstant: the constant already has a different value" name)))
  (set name value)
  (%put name '%constant t)
  name)

;;; `(constantp form [environment])` — is FORM's value always the same? True
;;; of anything but a symbol or a list, of `(quote x)`, of keywords, `t` and
;;; `nil`, and of a `defconstant`ed name. Nothing more clever: CLHS allows an
;;; implementation to recognise more, and requires only these.
(defun constantp (form &optional environment)
  environment
  (cond ((symbolp form)
         (or (null form) (eq form t) (keywordp form) (if (get form '%constant) t nil)))
        ((consp form) (and (eq (car form) 'quote) (consp (cdr form)) (null (cddr form))))
        (t t)))
