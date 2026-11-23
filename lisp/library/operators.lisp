;;;; The last three special operators, and the environment (#53, step I of #39).
;;;;
;;;; Common Lisp has 25 special operators; tier 0 had 22. `progv`,
;;;; `load-time-value` and `symbol-macrolet` were left out of tier 0 on
;;;; purpose (`docs/design.md`), and the interpreter never learns them. CLHS
;;;; 3.1.2.1.2.2 lets an implementation write a special operator as a macro,
;;;; and two of them are macros here; `symbol-macrolet` is in the compiler,
;;;; because only the compiler knows what is in scope.
;;;;
;;;; Decided in #53 (Carsten):
;;;;
;;;; 1. `progv` binds through one new instruction, `DBIND-SYMBOL` — `DBIND`
;;;;    with the symbol from the stack — one name per level of a recursion
;;;;    here, so every binding is an ordinary record that `UNMARK`, the
;;;;    unwinder, an error and an abandoned computation all restore.
;;;; 2. `load-time-value` evaluates while it expands: Quokka's `eval` is
;;;;    compile-then-call, so expansion time is compile time, the moment CLHS
;;;;    gives for code processed by `compile`.
;;;; 3. Macros see a real lexical environment through `&environment`, and
;;;;    `macroexpand-1` takes it, so `(incf x)` on a symbol macro evaluates
;;;;    its subforms once.

;;; ------------------------------------------------------------------------
;;; progv

;;; `(progv symbols values body...)`: bind each symbol dynamically to the
;;; matching value for BODY. The body becomes a function so the bindings
;;; can be made by a recursion of any depth before it runs; it is a closure,
;;; so it still sees the lexical variables around the `progv`.
(defmacro progv (symbols values &body body)
  `(%progv ,symbols ,values (lambda () ,@body)))

;;; One binding per level, then the body at the bottom. A name with no value
;;; left is bound and then made unbound, as CLHS says: the binding is what
;;; puts its old value back afterwards.
(defun %progv (symbols values body)
  (if (null symbols)
      (funcall body)
      (%dynamic-bind (car symbols) (car values)
        (if (null values) (makunbound (car symbols)))
        (%progv (cdr symbols) (cdr values) body))))

;;; ------------------------------------------------------------------------
;;; load-time-value

;;; `(load-time-value form [read-only-p])`: FORM's value, computed once and
;;; then a constant of the code. Evaluated here, while the macro expands —
;;; which is while the surrounding code is compiled. A macro that expanded
;;; the same form twice (one that `macroexpand`s its argument and then
;;; compiles it) would evaluate it twice; CLHS leaves how often to the
;;; implementation for code processed by `eval`. READ-ONLY-P is allowed to be
;;; ignored, and is.
(defmacro load-time-value (form &optional read-only-p)
  read-only-p
  (list 'quote (eval form)))

;;; ------------------------------------------------------------------------
;;; Macro expansion in an environment

;;; `(macroexpand-1 form [environment])`. The prelude's takes no environment
;;; and sees only global macros. This one sees the symbol macros and local
;;; macros of ENVIRONMENT, and runs an expander *in* it, so an expander that
;;; asks for `&environment` gets this one. No environment is the null
;;; lexical environment, as in CL: only global macros.
;;;
;;; The compiler no longer calls `macroexpand-1` (#53), so redefining it here
;;; cannot change how the compiler compiles itself.
(defun macroexpand-1 (form &optional environment)
  (cond ((symbolp form)
         (let ((definition (%environment-symbol-macro environment form)))
           (if definition
               (values (cdr definition) t)
               (values form nil))))
        ((and (consp form) (symbolp (car form)))
         (let ((expander (cond ((%environment-macro environment (car form)))
                               ;; A local function shadows a global macro.
                               ((%environment-function-p environment (car form)) nil)
                               (t (%macro-function (car form))))))
           (if expander
               (values (%expand expander (cdr form)
                                (if environment (second environment) nil)
                                (if environment (third environment) nil))
                       t)
               (values form nil))))
        (t (values form nil))))

;;; Expand until the head is no longer a macro, and say whether anything
;;; happened.
(defun macroexpand (form &optional environment)
  (labels ((walk (f changed)
             (multiple-value-bind (once expanded) (macroexpand-1 f environment)
               (if expanded (walk once t) (values f changed)))))
    (walk form nil)))

;;; `(define-symbol-macro symbol expansion)` (#68): SYMBOL, wherever no
;;; lexical binding hides it, stands for EXPANSION, as a `symbol-macrolet`
;;; around everything would make it. The compiler keeps the table
;;; (`%global-symbol-macros`). A special variable or a constant cannot also
;;; be a symbol macro (CLHS).
(defmacro define-symbol-macro (symbol expansion)
  `(progn (%define-symbol-macro ',symbol ',expansion) ',symbol))

(defun %define-symbol-macro (symbol expansion)
  (if (or (not (symbolp symbol)) (%special-variable-p symbol) (constantp symbol))
      (error 'program-error "define-symbol-macro wants a symbol that is not a variable" symbol))
  (setf (gethash symbol %global-symbol-macros) (cons symbol expansion))
  symbol)
