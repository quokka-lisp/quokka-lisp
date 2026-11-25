;;;; Symbols and their cells (#63, step P of #39).
;;;;
;;;; A symbol is a small object with a name, a value cell, a function cell, a
;;;; property list and a home (chapter 5). Tier 0 could read and write the
;;;; first three; this file is the rest of CLHS chapter 10 that is about the
;;;; cells, and chapter 5's `fdefinition` and `macro-function`.
;;;;
;;;; Decided in #63 (Carsten): the property list through two primitives,
;;;; `symbol-plist` and `%set-symbol-plist`, with everything else here
;;;; (decision 1); `fboundp`, `fmakunbound` and `functionp` primitives,
;;;; because only Rust can empty a cell or tell a closure from a macro
;;;; (decision 2); `macro-function` asking the environment first (decision
;;;; 3). `defconstant` and `constantp` are in `specials.lisp`, beside
;;;; `defvar` (decision 4).

;;; ------------------------------------------------------------------------
;;; Property lists (CLHS 10.2)

;;; `(get symbol indicator)` — a property of SYMBOL, or DEFAULT. The plist
;;; alternates indicators and values, as `getf` reads one.
(defun get (symbol indicator &optional default)
  (getf (symbol-plist symbol) indicator default))

;;; `(setf (get symbol indicator) value)`: change the value if the property
;;; is there, or put a new one on the front of the list.
(defun %put (symbol indicator value)
  (%set-symbol-plist symbol (%putf (symbol-plist symbol) indicator value))
  value)

;;; DEFAULT is evaluated, as CLHS says, and has no other use here.
(defsetf get (symbol indicator &optional default) (value)
  `(progn ,default (%put ,symbol ,indicator ,value)))

(defsetf symbol-plist %set-symbol-plist)

;;; `(remprop symbol indicator)` — remove the property, destructively, and
;;; say whether it was there.
(defun remprop (symbol indicator)
  (let ((plist (symbol-plist symbol)))
    (cond ((null plist) nil)
          ((eq (car plist) indicator)
           (%set-symbol-plist symbol (cddr plist))
           t)
          (t (%remove-property (cdr plist) indicator)))))

;;; AFTER is the cons holding a value; the next property starts after it.
(defun %remove-property (after indicator)
  (let ((next (cdr after)))
    (cond ((null next) nil)
          ((eq (car next) indicator)
           (rplacd after (cddr next))
           t)
          (t (%remove-property (cdr next) indicator)))))

;;; `(get-properties plist indicators)` — the first property of PLIST whose
;;; indicator is one of INDICATORS: its indicator, its value, and the tail
;;; of PLIST it starts; or three nils.
(defun get-properties (plist indicators)
  (cond ((null plist) (values nil nil nil))
        ((member (car plist) indicators :test #'eq)
         (values (car plist) (cadr plist) plist))
        (t (get-properties (cddr plist) indicators))))

;;; ------------------------------------------------------------------------
;;; Function cells (CLHS 5.3)

;;; A function name is a symbol or `(setf symbol)` (CLHS 5.1.2.9, #78).
;;; Since #78 a setf function is the function of a symbol the prelude's
;;; `%setf-function-symbol` keeps, so each function below finds that symbol
;;; and asks the primitive about it. Asking does not make the symbol: only
;;; defining a setf function does. Anything else is not a function name at
;;; all, a `type-error` (#75).
(defun %check-function-name (name)
  (if (not (or (symbolp name) (%setf-function-name-p name)))
      (error 'type-error "a function name is a symbol or (setf symbol)" name))
  name)

;;; The symbol whose function cell holds NAME's definition, or nil for a
;;; setf function never defined.
(defun %function-cell (name)
  (if (symbolp (%check-function-name name))
      name
      (%setf-function-symbol-if-any (second name))))

(defun fboundp (name)
  (let ((symbol (%function-cell name)))
    (cond ((symbolp name) (%fboundp name))
          (symbol (%fboundp symbol))
          (t nil))))

(defun fmakunbound (name)
  (let ((symbol (%function-cell name)))
    (if (or symbol (symbolp name)) (%fmakunbound (if (symbolp name) name symbol)))
    name))

;;; `fdefinition` of a special operator is not an error: CLHS leaves its
;;; value to the implementation, and SBCL's is a function that refuses to be
;;; called, which is what this is.
(defun fdefinition (name)
  (let ((symbol (%function-cell name)))
    (cond ((and (symbolp name) (special-operator-p name))
           (lambda (&rest arguments)
             arguments
             (error 'undefined-function "a special operator is not a function" name)))
          ((and symbol (or (symbolp name) (%fboundp symbol))) (symbol-function symbol))
          (t (error 'undefined-function "undefined function" name)))))

(defun %set-fdefinition (name function)
  (%set-symbol-function (%function-name-symbol (%check-function-name name)) function))

(defsetf fdefinition %set-fdefinition)

;;; `(macro-function symbol [environment])` — SYMBOL's expander, as CL calls
;;; one: a function of the form and the environment. A local `macrolet`
;;; macro in ENVIRONMENT hides a global one of the same name (#53). A macro
;;; object is called the same way since #76, but it is not itself a function
;;; (`functionp` says no), so it is handed back wrapped in one.
(defun macro-function (symbol &optional environment)
  (let ((expander (or (%environment-macro environment symbol) (%macro-function symbol))))
    (if expander
        (lambda (form env) (%expand expander form env))
        nil)))

;;; `(setf (macro-function symbol) function)` — FUNCTION, a function of the
;;; form and the environment as CL defines one, becomes SYMBOL's macro: a
;;; macro object that calls it, since #76 with the same two arguments.
(defun %set-macro-function (symbol function)
  (%set-symbol-function
   symbol
   (%make-macro (lambda (form environment) (funcall function form environment))))
  function)

(defsetf macro-function (symbol &optional environment) (function)
  `(progn ,environment (%set-macro-function ,symbol ,function)))

;;; `(copy-symbol symbol [copy-props])` — a new uninterned symbol of the
;;; same name; with COPY-PROPS, its value, function and a copy of its
;;; property list too (#68).
(defun copy-symbol (symbol &optional copy-props)
  (let ((copy (make-symbol (symbol-name symbol))))
    (if copy-props
        (progn
          (if (boundp symbol) (set copy (symbol-value symbol)))
          (if (fboundp symbol) (%set-symbol-function copy (symbol-function symbol)))
          (%set-symbol-plist copy (copy-list (symbol-plist symbol)))))
    copy))

;;; The number the next `gensym` puts in its name (CLHS 10.2). `gensym`
;;; itself is in `packages.lisp`, which loads last; the variable is here, so
;;; that it is special before `rebuild.lisp` binds it (step Y, #72).
(defvar *gensym-counter* 0)
