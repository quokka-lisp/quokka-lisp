;;;; eval, load, and the loop a host drives (#47, step D of #39).
;;;;
;;;; With the reader (#45) and the printer (#46) in Lisp, the last piece of a
;;;; read-eval-print loop is the E. Quokka never interprets in a built world
;;;; (#25): `eval` compiles its form with the image's own compiler and calls
;;;; the result, which is what SBCL does in its default `:compile` evaluator
;;;; mode.
;;;;
;;;; Decided in #47 (Carsten): the loop itself is Lisp too. A host — the REPL,
;;;; the differential harness, later the browser console — hands this file a
;;;; string with `%host-source` and calls `%host-step` until it says the input
;;;; is finished. Each call reads one form with the Lisp reader, so it is
;;;; interned through `*package*`; evaluates it; and prints the value with
;;;; `prin1`. The host only relays what it is told.

;;; `(eval form)` — compile FORM into a function of no arguments and call it.
;;;
;;; An `eval` inside a macro expander runs while another form is being
;;; compiled, and so compiles inside a compilation; `compile-form` saves and
;;; restores the outer one's work around it (#47). Two *processes* compiling
;;; at once is a different matter, and phase 3's (#34).
(defun eval (form)
  (funcall (compile-form form)))

;;; ------------------------------------------------------------------------
;;; compile (step R, #65)
;;;
;;; In a built world every function is already compiled: the interpreter is
;;; not in the machine at all (#29), and `eval` compiles before it runs. So
;;; `compile` has one job left, turning a lambda expression into a function,
;;; and `compiled-function-p` is true of every function there is.

;;; `(compile name [definition])` — DEFINITION, a lambda expression, compiled
;;; in the null lexical environment; or, without one, NAME's present
;;; definition, which is compiled already and is returned as it is (there is
;;; no source to compile it from again, #55). A non-nil NAME is given the
;;; result: as its macro function if it names a macro, as its function
;;; otherwise. The values are NAME (or the function, if NAME is nil), and
;;; nil, nil: Quokka's compiler has no warnings or failures to report that it
;;; does not signal as errors.
(defun compile (name &optional (definition nil definition-p))
  (let ((function (cond ((not definition-p)
                         (or (macro-function name) (symbol-function name)))
                        ((functionp definition) definition)
                        (t (eval (list 'function definition))))))
    (cond ((null name) (values function nil nil))
          (t (if definition-p
                 (if (macro-function name)
                     (setf (macro-function name) function)
                     (setf (fdefinition name) function)))
             (values name nil nil)))))

(defun compiled-function-p (object) (functionp object))

;;; The lambda expression a function was made from, whether it closes over
;;; anything, and its name. CLHS lets an implementation always answer that
;;; the expression is unknown; definitions keep no source until #55 (#68).
(defun function-lambda-expression (function)
  (if (not (functionp function)) (error 'type-error "a function" function))
  (values nil t nil))

;;; Compiler macros (decision 1): the compiler applies them, from its table
;;; `%compiler-macros` (compile.lisp); these are CL's ways of filling it.
;;; `(define-compiler-macro name lambda-list body...)` takes a macro lambda
;;; list, whose `&whole` is the call form and whose `&environment` is the
;;; environment; the rest of it is matched against the call's arguments —
;;; after `#'name` when the call is `(funcall #'name ...)`.
(defmacro define-compiler-macro (name lambda-list &body body)
  (let ((form (or (%lambda-list-marker '&whole lambda-list) (gensym)))
        (environment (or (%lambda-list-marker '&environment lambda-list) (gensym))))
    `(progn
       (setf (compiler-macro-function ',name)
             (lambda (,form ,environment)
               ,environment
               (destructuring-bind ,(%without-markers lambda-list)
                   (if (eq (car ,form) 'funcall) (cddr ,form) (cdr ,form))
                 ,@body)))
       ',name)))

;;; The variable after MARKER (`&whole`, `&environment`) in LAMBDA-LIST, or nil.
(defun %lambda-list-marker (marker lambda-list)
  (let ((tail (member marker lambda-list)))
    (if tail (second tail) nil)))

;;; LAMBDA-LIST without `&whole VAR` and `&environment VAR`.
(defun %without-markers (lambda-list)
  (cond ((null lambda-list) nil)
        ((atom lambda-list) lambda-list)
        ((member (car lambda-list) '(&whole &environment))
         (%without-markers (cddr lambda-list)))
        (t (cons (car lambda-list) (%without-markers (cdr lambda-list))))))

(defun compiler-macro-function (name &optional environment)
  environment
  (values (gethash name %compiler-macros)))

(defun %set-compiler-macro-function (name function)
  (if function
      (setf (gethash name %compiler-macros) function)
      (remhash name %compiler-macros))
  function)

(defsetf compiler-macro-function (name &optional environment) (function)
  `(progn ,environment (%set-compiler-macro-function ,name ,function)))

;;; The 25 special operators of CLHS 3.1.2.1.2.1. Quokka writes `progv` and
;;; `load-time-value` as macros (#53), which CLHS allows; they are special
;;; operators all the same. Quokka's own `%special-operator-p` answers for the
;;; compiler, which has operators of its own (`lambda`, `%handler-case`).
(defvar %special-operators
  '(block catch eval-when flet function go if labels let let* load-time-value
    locally macrolet multiple-value-call multiple-value-prog1 progn progv quote
    return-from setq symbol-macrolet tagbody the throw unwind-protect))

(defun special-operator-p (symbol)
  (if (symbolp symbol)
      (if (member symbol %special-operators :test #'eq) t nil)
      (error 'type-error "special-operator-p takes a symbol" symbol)))

;;; A fresh cons, so no form in a file can be mistaken for the end of it.
(defvar %end-of-input (list :end-of-input))

;;; `(load source)` — every form of SOURCE, read and evaluated in order. A
;;; string names a file, which is fetched through the host (`%read-source`,
;;; #35); an input stream is read directly. `*package*` and `*readtable*` are
;;; rebound for the duration, as CLHS says, so an `in-package` in a file stays
;;; in that file. CLHS's keyword arguments — `:verbose`, `:print`,
;;; `:if-does-not-exist` — are not here yet.
(defun load (source)
  (let ((stream (if (stringp source)
                    (make-string-input-stream (%read-source source))
                    source))
        (*package* *package*)
        (*readtable* *readtable*))
    (%load-forms stream)
    t))

(defun %load-forms (stream)
  (let ((form (read stream nil %end-of-input)))
    (if (eq form %end-of-input)
        nil
        (progn (eval form)
               (%load-forms stream)))))

;;; ------------------------------------------------------------------------
;;; The host's loop

(defvar %host-input (make-string-input-stream ""))

;;; Give the loop its input: the line a person typed, or a whole file.
(defun %host-source (text)
  (setq %host-input (make-string-input-stream text))
  t)

;;; Read, evaluate and print one form, and say what happened:
;;;
;;;   (:value "printed value")   the form's value, as `prin1` writes it
;;;   (:error "message")         a Lisp error, caught here
;;;   (:eof)                     nothing left to read
;;;
;;; An error is caught so that the host can carry on with the next form, as a
;;; REPL does. What cannot be caught here — the step budget running out — the
;;; host sees as a failure of the call itself. The argument is ignored: the
;;; host's trampoline always passes one.
(defun %host-step (ignored)
  (handler-case
      (let ((form (read %host-input nil %end-of-input)))
        (if (eq form %end-of-input)
            (list :eof)
            (list :value (prin1-to-string (eval form)))))
    (error (condition) (list :error (%condition-text condition)))))

;;; A condition's message as text, for a person to read.
(defun %condition-text (condition)
  (let ((message (%condition-message condition)))
    (if (stringp message) message (princ-to-string message))))

;;; ------------------------------------------------------------------------
;;; The implementation's limits (step Y, #72)

;;; Each is an *exclusive* upper bound, and each is the bytecode's real
;;; ceiling (`docs/bytecode.md` §9), measured rather than assumed: a call
;;; written out in the source passes at most 255 arguments (`CALL`'s count
;;; is a byte), and a lambda list names at most 256 variables (a scope's).
;;; `apply` can pass more, and `values` return more, but a limit is a
;;; promise about what always works, so it is the smaller number. SBCL's are
;;; all 2^62 - 1.
(defconstant call-arguments-limit 256)
(defconstant lambda-parameters-limit 256)
(defconstant multiple-values-limit 256)

;;; The lambda-list keywords CLHS defines; SBCL's list adds its own
;;; `sb-int:&more`, which is not ours to copy.
(defconstant lambda-list-keywords
  '(&allow-other-keys &aux &body &environment &key &optional &rest &whole))

;;; ------------------------------------------------------------------------
;;; lambda and defmacro are macros

;;; CLHS makes `lambda` and `defmacro` macros. Quokka's compiler compiles
;;; both forms itself, as the interpreter evaluates them, so these
;;; expanders are never used by it; they are what `macro-function` and
;;; `macroexpand` find (#80). `lambda` expands as SBCL's does, to
;;; `(function (lambda ...))`. `defmacro` expands to what the compiler
;;; compiles a `defmacro` form into, with `%macro-lambda` for the expander.
(%set-symbol-function
 'lambda
 (%make-macro (lambda (form environment)
                environment
                (list 'function form))))

(%set-symbol-function
 'defmacro
 (%make-macro (lambda (form environment)
                environment
                (destructuring-bind (name lambda-list &body body) (cdr form)
                  `(progn
                     (%set-symbol-function ',name (%make-macro (%macro-lambda ,lambda-list ,@body)))
                     ',name)))))
