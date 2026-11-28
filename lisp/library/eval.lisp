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
;;;
;;; **Top-level forms** (#104, CLHS 3.2.3.1). Compiling a whole form before
;;; running any of it is wrong for one kind of form: a `progn` whose early
;;; parts define what its later parts use.
;;;
;;;     (progn (defmacro m () 1) (m))
;;;
;;; compiled whole, `(m)` is a call to a function, because no macro M exists
;;; until the code runs. CLHS says the parts of a top-level `progn` are
;;; top-level forms themselves, and SBCL's `eval` treats them so: each is
;;; evaluated before the next is looked at. It matters because a defining
;;; macro expands into exactly this — `(progn (defstruct ...) ...)`.
;;;
;;; So `eval` expands the form's macros until its head is no macro, and
;;; then: the forms of a `progn` are evaluated in turn; so are those of an
;;; `eval-when` that names `:execute`; and so are those of a `locally`,
;;; `macrolet` or `symbol-macrolet`, each wrapped in the same declarations
;;; and definitions — `(macrolet defs a b)` is `(macrolet defs a)` and then
;;; `(macrolet defs b)`. Anything else is compiled whole, as before.
;;;
;;; One limit, deliberate: inside a `macrolet`, a single body form is taken
;;; apart only if it is written as a `progn`, not if a local macro would
;;; expand into one. Seeing that needs the macrolet's environment, which
;;; only the compiler builds.
;;;
;;; CONTEXT is the form as it was written, carried down so that a compile
;;; error can say where it is: by the time anything is compiled, a `defun`
;;; has become a `%set-symbol-function` inside a `progn`, and "in
;;; %SET-SYMBOL-FUNCTION" would name nothing the programmer wrote (#110).
;;;
;;; A defining form evaluated here is recorded as a definition of the world
;;; (`definitions.lisp`, #55), with the text `load` or the host loop found
;;; for it. The text is taken for this form alone: an `eval` inside it, in a
;;; macro's expander say, sees none.
;;; ------------------------------------------------------------------------
;;; Warnings for what is named and not defined (#110)
;;;
;;; A definition that calls a function nobody has defined, or reads a
;;; variable nobody has, is usually a slip of the fingers, and a compiler
;;; that says nothing leaves it to be found when the code runs. The
;;; compiler notes what each definition refers to (`%references`); what is
;;; still undefined when the *unit* ends is warned about, once a name. A
;;; unit is what a person would call one go: the text a host hands over (a
;;; line at a terminal, a whole file given to `quokka build`), a file given
;;; to `load`, an entry in the listener. So two functions that call each
;;; other draw no warning, as they do not in SBCL, where a file is a unit
;;; (`with-compilation-unit`).
;;;
;;; An undefined function is a `style-warning`, as in SBCL, and an
;;; undefined variable a `warning`. They are printed on `*error-output*`,
;;; which a host keeps apart from what the program prints.

;;; What definitions in this unit named: `(kind . name)`, newest first.
(defvar %undefined-pending nil)

(defun %note-undefined (references)
  (when references
    (dolist (name (car (cdr references)))
      (unless (%function-known-p name)
        (pushnew (cons :function name) %undefined-pending :test #'equal)))
    (dolist (name (cdr (cdr references)))
      (unless (%variable-known-p name)
        (pushnew (cons :variable name) %undefined-pending :test #'equal)))))

(defun %function-known-p (name)
  (or (not (symbolp name)) (fboundp name) (special-operator-p name)))

(defun %variable-known-p (name)
  (or (boundp name) (constantp name) (%special-variable-p name)))

;;; The unit is over: warn of what it named that is still not defined.
(defun %warn-undefined ()
  (let ((pending (reverse %undefined-pending)))
    (setq %undefined-pending nil)
    (dolist (entry pending)
      (if (eq (car entry) :function)
          (unless (%function-known-p (cdr entry))
            (warn 'style-warning "undefined function" (cdr entry)))
          (unless (%variable-known-p (cdr entry))
            (warn 'warning "undefined variable" (cdr entry)))))))

(defun eval (form)
  (let ((place %definition-place) (source %definition-source)
        ;; Around a defining form the compiler is asked what it refers to
        ;; (`%references`, a compiler global, put back after).
        (references (if (and %recording-definitions (%definition-key form))
                        (cons nil (cons nil nil))
                        nil))
        (outer %references))
    (setq %references references)
    (unwind-protect
         (multiple-value-prog1
             (let ((%definition-place nil) (%definition-source nil)
                   (%part-source source))
               (%eval-in form form))
           (setq %references outer)
           (%note-undefined references)
           (%record-definition form place source references))
      (setq %references outer))))

;;; Where the form being evaluated came from, for the definitions among the
;;; parts of a top-level `progn`, which are recorded one by one (`%eval-part`).
(defvar %part-source nil)

(defun %eval-in (form context)
  (let ((form (macroexpand form)))
    (cond ((not (consp form)) (funcall (%compile-in context form)))
          ((eq (car form) 'progn) (%eval-each (cdr form) nil context))
          ((eq (car form) 'eval-when)
           (if (or (member :execute (cadr form)) (member 'eval (cadr form)))
               (%eval-each (cddr form) nil context)
               nil))
          ((or (eq (car form) 'locally) (eq (car form) 'macrolet)
               (eq (car form) 'symbol-macrolet))
           (%eval-binder form context))
          (t (funcall (%compile-in context form))))))

;;; Compile FORM, with CONTEXT as the form any error is said to be in. Bound
;;; for the compilation only: the function is called after the binding is
;;; gone, so a program's own `eval` does not inherit it.
(defvar %toplevel-form nil)

(defun %compile-in (context form)
  (let ((%toplevel-form context))
    (compile-form form)))

;;; Evaluate FORMS in turn, each inside PREFIX if there is one — the head of
;;; a `locally` or `macrolet` and what it declares or defines. The values are
;;; the last form's.
(defun %eval-each (forms prefix context)
  (if (null forms)
      nil
      (let ((rest forms))
        (tagbody
         next
           (if (cdr rest)
               (progn (%eval-part (%eval-wrap (car rest) prefix) context)
                      (setq rest (cdr rest))
                      (go next))))
        (%eval-part (%eval-wrap (car rest) prefix) context))))

;;; Evaluate one part of a top-level `progn`. A part that defines something
;;; is recorded as a definition of its own, with what it refers to, when the
;;; form around it is not itself a definition: `(progn (defun a ...) (defun
;;; b ...))`, or a program's own macro that expands into one. They were
;;; recorded without their references, so `who-calls` did not know them and
;;; a redefined macro never marked them stale (#184). The parts of a
;;; defining form, the `defun`s a `defstruct` expands into, belong to that
;;; definition and are not recorded again.
(defun %eval-part (form context)
  (if (and %recording-definitions (null %references) (%definition-key form))
      (let ((references (cons nil (cons nil nil))))
        (setq %references references)
        (unwind-protect
             (multiple-value-prog1 (%eval-in form context)
               (setq %references nil)
               (%note-undefined references)
               (%record-definition form nil %part-source references))
          (setq %references nil)))
      (%eval-in form context)))

(defun %eval-wrap (form prefix)
  (if prefix (append prefix (list form)) form))

;;; A top-level `locally`, `macrolet` or `symbol-macrolet`. Its body is
;;; split when there is something to split: several forms, or one `progn`.
;;; Otherwise it is compiled whole — which is also what ends the recursion,
;;; since each piece is a binder around a single form.
(defun %eval-binder (form context)
  (let* ((locally (eq (car form) 'locally))
         (body (if locally (cdr form) (cddr form)))
         (prefix (if locally (list 'locally) (list (car form) (cadr form)))))
    (tagbody
     next
       (if (and (consp body) (consp (car body)) (eq (car (car body)) 'declare))
           (progn (setq prefix (append prefix (list (car body))))
                  (setq body (cdr body))
                  (go next))))
    (cond ((and body (cdr body)) (%eval-each body prefix context))
          ((and body (consp (car body)) (eq (car (car body)) 'progn))
           (%eval-each (cdr (car body)) prefix context))
          (t (funcall (%compile-in context form))))))

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

;;; A generic function is a funcallable instance, and SBCL says it is not a
;;; compiled function (#124).
(defun compiled-function-p (object)
  (and (functionp object) (not (%funcallable-instance-p object))))

;;; The lambda expression a function was made from, whether it closes over
;;; anything, and its name. CLHS lets an implementation always answer that
;;; the expression is unknown, which Quokka did until definitions kept
;;; their source (#55): a global function whose `defun` the world recorded
;;; answers with its lambda expression, if it is still the function that
;;; `defun` made. It may close over the `defun`'s lexical environment, so
;;; the second value is true.
(defun function-lambda-expression (function)
  (if (not (functionp function)) (error 'type-error :datum function :expected-type 'function))
  (let* ((name (if (%generic-function-p function)
                   (%gf-name function)
                   ;; The compiler assembles an anonymous function under the
                   ;; name LAMBDA, which names nothing.
                   (let ((name (%function-name function))) (if (eq name 'lambda) nil name))))
         (definition (and name (symbolp name) (find-definition name :function))))
    (if (and definition (eq (definition-object definition) function))
        (values (%defun-lambda (definition-form definition)) t name)
        (values nil t name))))

;;; `(defun name lambda-list . body)` as the lambda expression of its
;;; function: the body in the implicit block `defun` puts it in, after its
;;; documentation and declarations, as SBCL gives it.
(defun %defun-lambda (form)
  (let ((prefix nil) (body (cdddr form)))
    (do () ((not (or (and (consp (car body)) (eq (car (car body)) 'declare))
                     (and (stringp (car body)) (cdr body)))))
      (push (car body) prefix)
      (setq body (cdr body)))
    `(lambda ,(third form) ,@(nreverse prefix)
       (block ,(let ((name (second form))) (if (consp name) (second name) name)) ,@body))))

;;; Compiler macros (decision 1): the compiler applies them, from its table
;;; `%compiler-macros` (compile.lisp); these are CL's ways of filling it.
;;; `(define-compiler-macro name lambda-list body...)` takes a macro lambda
;;; list, whose `&whole` is the call form and whose `&environment` is the
;;; environment; the rest of it is matched against the call's arguments —
;;; after `#'name` when the call is `(funcall #'name ...)`.
(defmacro define-compiler-macro (name lambda-list &body body)
  (let* ((form (or (%lambda-list-marker '&whole lambda-list) (gensym)))
         (environment (or (%lambda-list-marker '&environment lambda-list) (gensym)))
         ;; The body is in a block named after the function, as a macro's
         ;; is (CLHS 3.2.2.1.1); for (setf name), the block is NAME.
         (block-name (if (consp name) (second name) name))
         (body (if (and (stringp (car body)) (cdr body)) (cdr body) body)))
    `(progn
       (setf (compiler-macro-function ',name)
             (lambda (,form ,environment)
               ,environment
               (destructuring-bind ,(%without-markers lambda-list)
                   (if (eq (car ,form) 'funcall) (cddr ,form) (cdr ,form))
                 ,@(%body-declarations body)
                 (block ,block-name ,@(%body-forms body)))))
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

;;; A compiler macro for (setf NAME) is kept under the symbol that holds
;;; NAME's setf function (chapter 66), since the table compares keys with
;;; `eq` and every (setf NAME) a program writes is a new list.
(defun %compiler-macro-key (name)
  (if (consp name) (%setf-function-symbol (second name)) name))

(defun compiler-macro-function (name &optional environment)
  environment
  (values (gethash (%compiler-macro-key name) %compiler-macros)))

(defun %set-compiler-macro-function (name function)
  (if function
      (setf (gethash (%compiler-macro-key name) %compiler-macros) function)
      (remhash (%compiler-macro-key name) %compiler-macros))
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
;;;
;;; `*load-pathname*` and `*load-truename*` are bound to the name of the
;;; file being loaded, so that a file can find what lies beside it. A
;;; string, not a pathname: pathnames are tier 2. Nil outside a `load`, and
;;; when loading from a stream.
(defvar *load-pathname* nil)
(defvar *load-truename* nil)

(defun load (source)
  (let ((stream (if (stringp source)
                    (make-string-input-stream (%read-source source))
                    source))
        (*load-pathname* (if (stringp source) source nil))
        (*load-truename* (if (stringp source) source nil))
        (*package* *package*)
        (*readtable* *readtable*))
    (%load-forms stream)
    ;; A file is a unit: what it named and never defined is said now.
    (%warn-undefined)
    t))

(defun %load-forms (stream)
  (let* ((start (%stream-position stream))
         (form (read stream nil %end-of-input)))
    (if (eq form %end-of-input)
        nil
        (progn (%eval-read-form form stream start *load-truename*)
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
;;; Each form runs inside an `abort` restart, CLHS's way back to the top
;;; level (#125, decision 5). An error nobody handles goes to the debugger,
;;; which calls a program's `*debugger-hook*` and then, there being no
;;; debugger yet (step F), invokes the innermost `abort` restart: this one,
;;; which tells the host what went wrong so that it can carry on with the
;;; next form, as a REPL does. Until step B the form ran inside a
;;; `handler-case` that caught every error, so an error never reached the
;;; debugger, nor a program's hook. What cannot be caught at all, the step
;;; budget running out, the host sees as a failure of the call itself. The
;;; argument is ignored: the host's trampoline always passes one.
;;; Put right the variables a person's own form can leave unusable, as
;;; SBCL puts `*package*` right: with `*package*` set to 5 nothing could be
;;; read again, the form that would mend it included, and every line after
;;; said only that it was aborted; with the debugger on, its banner failed
;;; while printing and opened another debugger, 176 deep (#181). The host
;;; loop does this before each read, and the debugger as it opens.
(defun %repair-syntax ()
  (unless (packagep *package*)
    (setq *package* (find-package "COMMON-LISP-USER")))
  (unless (readtablep *readtable*)
    (setq *readtable* (copy-readtable nil)))
  (unless (and (integerp *print-base*) (<= 2 *print-base* 36))
    (setq *print-base* 10))
  (unless (and (integerp *read-base*) (<= 2 *read-base* 36))
    (setq *read-base* 10))
  nil)

(defun %host-step (ignored)
  (if (null %host-input)
      ;; The input was given up (below): the host is told it has ended.
      (list :eof)
      (%host-step-reading)))

(defun %host-step-reading ()
  (%repair-syntax)
  (let ((%aborted-condition nil)
        (start (%stream-position %host-input))
        (reading t))
    (restart-case
        ;; The restart is told to the host loop before anything is read, so
        ;; that a reader's error is reported like any other (#182).
        (let* ((%host-abort-restart (find-restart 'abort))
               (form (read %host-input nil %end-of-input)))
          (setq reading nil)
          (if (eq form %end-of-input)
              ;; The end of what the host handed over ends the unit.
              (progn (%warn-undefined) (list :eof))
              (list :value (prin1-to-string (%eval-read-form form %host-input start nil)))))
      (abort ()
        :report "Return to the top level."
        ;; A read that failed without taking a character would fail the
        ;; same way for ever, at the end of the input too, and the host
        ;; would never be told `(:eof)`: `(setq *readtable* nil)` hung the
        ;; terminal (#180). The input is given up, until the host brings
        ;; more (`%host-source`).
        (when (and reading (eql (%stream-position %host-input) start))
          (setq %host-input nil))
        (if %aborted-condition
            (list :error (%condition-text %aborted-condition)
                  (%condition-type-name %aborted-condition))
            (list :error "Aborted." "ERROR"))))))

;;; A condition's type as text, for a host that wants to say which kind of
;;; error a form ended in (#103).
(defun %condition-type-name (condition)
  (let ((type (%condition-type-of condition)))
    (if (symbolp type) (symbol-name type) "ERROR")))

;;; Does TEXT end outside every form? A terminal REPL asks before it
;;; evaluates a line, and reads another line when the answer is no (#119).
;;; Only this reader can say: a parenthesis in a string, a comment or `#\(`
;;; closes nothing. `*read-suppress*` reads without interning or evaluating
;;; `#.`, so asking changes nothing. Input that ends inside a form is the one
;;; case that wants more text; any other fault is whole enough to evaluate,
;;; and evaluating it is what reports the fault.
(defun %source-complete-p (text)
  (let ((in (make-string-input-stream text))
        (*read-suppress* t))
    (handler-case
        (tagbody
         again
           (if (eq (read in nil %end-of-input) %end-of-input)
               (return-from %source-complete-p t))
           (go again))
      (end-of-file () nil)
      (error () t))))

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
                     (%set-symbol-function
                      ',name
                      ;; In a block named after the macro, as the compiler's
                      ;; own `defmacro` gives it (#109).
                      (%make-macro (%macro-lambda ,lambda-list ,@(%block-body name body))))
                     ',name)))))

;;; ------------------------------------------------------------------------
;;; disassemble, and modules

;;; `(disassemble fn)` — the bytecode of a function, written to standard
;;; output (CLHS: the format is the implementation's). FN is a function, a
;;; function name or a lambda expression, which is compiled first; a macro's
;;; name shows its expander. The machine's own disassembler does the work,
;;; as the bootstrap REPL's `--show-code` does (#82). A primitive is Rust,
;;; with no bytecode to show.
(defun disassemble (fn)
  (let ((function (cond ((and (consp fn) (eq (car fn) 'lambda)) (compile nil fn))
                        ((functionp fn) fn)
                        ((and (symbolp fn) (macro-function fn)) (%macro-function fn))
                        (t (fdefinition fn)))))
    ;; `%disassemble` refuses anything that is not compiled code: here, a
    ;; primitive.
    (handler-case (write-string (%disassemble function))
      (type-error () (format t "; ~S is a primitive, written in Rust~%" fn)))
    nil))

;;; `*modules*`, `provide` and `require` (CLHS 24.1.2, deprecated there but
;;; present). A module is a name in `*modules*`. `require` loads the files it
;;; is given, by the names the host knows them by, when the module is not
;;; there yet; without files it cannot know where a module lives, and says
;;; so, as SBCL does for a module it does not know.
(defvar *modules* nil)

(defun provide (module-name)
  (let ((name (string module-name)))
    (if (not (member name *modules* :test #'string=)) (push name *modules*))
    t))

(defun require (module-name &optional pathnames)
  (let ((name (string module-name)))
    (cond ((member name *modules* :test #'string=) nil)
          (pathnames
           (dolist (file (if (listp pathnames) pathnames (list pathnames)))
             (load file))
           t)
          (t (error "require: no module by this name is known, and no files were given"
                    module-name)))))
