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

;;; `fdefinition` is `symbol-function` for the names Quokka has: symbols.
;;; A name `(setf foo)` would need setf functions, which Quokka does not
;;; have (#52).
(defun fdefinition (name)
  (if (symbolp name)
      (symbol-function name)
      (error 'undefined-function)))

(defsetf fdefinition (name) (function)
  `(%set-symbol-function ,name ,function))

;;; `(macro-function symbol [environment])` — SYMBOL's expander, as CL calls
;;; one: a function of the form and the environment. A local `macrolet`
;;; macro in ENVIRONMENT hides a global one of the same name (#53). Quokka's
;;; own macro objects are called with the form's arguments spread out, so
;;; what is handed back is a function that does that.
(defun macro-function (symbol &optional environment)
  (let ((expander (or (%environment-macro environment symbol) (%macro-function symbol))))
    (if expander
        (lambda (form env)
          (%expand expander (cdr form)
                   (if env (second env) nil)
                   (if env (third env) nil)))
        nil)))

;;; `(setf (macro-function symbol) function)` — FUNCTION, a function of the
;;; form and the environment as CL defines one, becomes SYMBOL's macro. It is
;;; wrapped the other way: a macro object that gets the arguments, puts the
;;; form back together and asks for the environment it is expanding in.
(defun %set-macro-function (symbol function)
  (%set-symbol-function
   symbol
   (%make-macro (lambda (&rest arguments)
                  (funcall function (cons symbol arguments) (%current-environment)))))
  function)

(defsetf macro-function (symbol &optional environment) (function)
  `(progn ,environment (%set-macro-function ,symbol ,function)))
