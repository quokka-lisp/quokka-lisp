;;;; conditions.lisp — a condition is an instance of its class (phase 3,
;;;; step B, #125; the design is `docs/conditions.md`)
;;;;
;;;; Until this step a condition was the machine's own object, with a type,
;;;; a message and a list of data, and `clos.lisp` gave each standard
;;;; condition type a class with no slots. Here those classes get the slots
;;;; CLHS gives them, a program can define its own with `define-condition`,
;;;; and `make-condition` makes an instance: a `StandardInstance` whose class
;;;; is a `condition-class`, so slots, readers and `print-object` work on it
;;;; as on any instance (chapter 73). A condition is not a `standard-object`,
;;;; as in SBCL.
;;;;
;;;; Signalling is here too, in Lisp: `signal`, `error`, `handler-bind`,
;;;; restarts, and `%signal-machine-error`, through which the machine's own
;;;; errors arrive as instances of these classes.

;;; ------------------------------------------------------------------------
;;; Making conditions

;;; `(make-condition type &rest initargs)`: a condition of TYPE, a condition
;;; class or its name, with its slots filled from INITARGS, the class's
;;; default initargs and the slots' initforms. As in SBCL, an initarg no
;;; slot takes is ignored: a condition has no `initialize-instance` methods
;;; whose keywords would make it valid.
(defun make-condition (type &rest initargs)
  (let ((class (if (symbolp type) (find-class type nil) type)))
    (unless (%condition-class-p class)
      (error 'type-error "make-condition wants a condition type" type))
    (unless (evenp (length initargs))
      (error "make-condition was given an odd number of initialization arguments" initargs))
    (%make-condition class initargs)))

(defun %condition-class-p (object)
  (and object (%instance-wrapper object) (eq (class-of object) (find-class 'condition-class))))

(defun %make-condition (class initargs)
  (let* ((class (%ensure-finalized class))
         (initargs (%add-default-initargs class initargs)))
    (apply #'%shared-initialize (%allocate-instance class) t initargs)))

;;; SBCL makes a condition with `make-instance` too.
(defmethod make-instance ((class condition-class) &rest initargs)
  (%make-condition class initargs))

;;; ------------------------------------------------------------------------
;;; Defining condition types

;;; `(define-condition name (parent-type...) (slot-spec...) option...)`
;;; (CLHS): a class of metaclass `condition-class`, whose slots are
;;; specified as `defclass`'s are. The options are `:default-initargs`,
;;; `:documentation` and `:report`; the report is kept with the class and
;;; printed by `print-object` (below).
(defmacro define-condition (name parent-types slot-specs &rest options)
  (let ((names (mapcar (lambda (spec) (if (consp spec) (car spec) spec)) slot-specs))
        (report nil) (class-options nil))
    (unless (= (length names) (length (remove-duplicates names)))
      (error 'program-error "two slots with the same name" name))
    (dolist (option options)
      (unless (and (consp option) (member (car option) '(:default-initargs :documentation :report)))
        (error 'program-error "an unknown define-condition option" option))
      ;; A second `:report` replaces the first, as in SBCL.
      (if (eq (car option) :report)
          (setq report (cadr option))
          (push option class-options)))
    `(progn
       (ensure-class ',name :metaclass 'condition-class
                     :direct-superclasses ',parent-types
                     :direct-slots (list ,@(mapcar #'%canonical-slot slot-specs))
                     ,@(%canonical-class-options (nreverse class-options)))
       (%set-condition-report (find-class ',name) ,(%report-form report))
       ',name)))

;;; A `:report` option as a form whose value is kept: a string as it is, a
;;; lambda expression as a function made where the definition is, a symbol
;;; as the name of a function, called by name when the condition is
;;; reported, so that it may be defined later.
(defun %report-form (report)
  (cond ((null report) nil)
        ((stringp report) report)
        ((symbolp report) `',report)
        ((and (consp report) (eq (car report) 'lambda)) `(function ,report))
        (t (error 'program-error "a :report is a string, a function name or a lambda expression"
                  report))))

;;; A class's own report, on its property list; a redefinition without one
;;; leaves it to inherit its parents' again.
(defun %set-condition-report (class report)
  (%set-class-plist class (if report
                              (list* '%report report (%remove-plist-key (%class-plist class) '%report))
                              (%remove-plist-key (%class-plist class) '%report))))

;;; ------------------------------------------------------------------------
;;; The standard condition types' slots (CLHS 9)
;;;
;;; The classes exist already, with the parents the prelude's lattice gives
;;; them (`clos.lisp`); defining them again here gives them their slots and
;;; keeps the classes. Only the types that have slots of their own are
;;; listed. The slots are named in package QUOKKA, as SBCL's are in
;;; SB-KERNEL: a program reaches them through the readers CLHS names.
;;;
;;; `condition` itself has two slots no standard names: the message and
;;; data of a condition the library signals its own way,
;;; `(error 'program-error "message" data...)`, which SBCL would refuse but
;;; about 240 calls in the library and every compiler error use. Such a
;;; condition reports its message.

(ensure-class 'condition :metaclass 'condition-class :direct-superclasses (list (find-class t))
              :direct-slots (list (list :name '%message :initform nil :initfunction (lambda () nil))
                                  (list :name '%data :initform nil :initfunction (lambda () nil))))

(define-condition simple-condition (condition)
  ((format-control :initarg :format-control :initform nil
                   :reader simple-condition-format-control)
   (format-arguments :initarg :format-arguments :initform nil
                     :reader simple-condition-format-arguments)))

(define-condition type-error (error)
  ((datum :initarg :datum :reader type-error-datum)
   (expected-type :initarg :expected-type :reader type-error-expected-type)))

(define-condition cell-error (error)
  ((name :initarg :name :reader cell-error-name)))

(define-condition unbound-slot (cell-error)
  ((instance :initarg :instance :reader unbound-slot-instance)))

(define-condition package-error (error)
  ((package :initarg :package :reader package-error-package)))

(define-condition stream-error (error)
  ((stream :initarg :stream :reader stream-error-stream)))

(define-condition file-error (error)
  ((pathname :initarg :pathname :reader file-error-pathname)))

(define-condition print-not-readable (error)
  ((object :initarg :object :reader print-not-readable-object)))

(define-condition arithmetic-error (error)
  ((operation :initarg :operation :reader arithmetic-error-operation)
   (operands :initarg :operands :initform nil :reader arithmetic-error-operands)))

;;; ------------------------------------------------------------------------
;;; Reports

;;; A condition prints as `#<TYPE-ERROR>` when escaped, and as its report
;;; when not, which is what `princ` and `~A` show (CLHS 9.1.3). The report
;;; is the message of a condition the library made its own way, then the
;;; nearest report in the class precedence list, then CLHS's default. One
;;; of the machine's own conditions prints as the printer has always printed
;;; it, `#<SIMPLE-ERROR: message>`, by the method on `t`, unless it has a
;;; message to report: it has no slots, so no class report can read it.
(defmethod print-object ((condition condition) stream)
  (cond ((%conditionp condition)
         (if (or *print-escape* (not (stringp (%condition-message condition))))
             (call-next-method)
             (write-string (%condition-message condition) stream)))
        (*print-escape*
         (print-unreadable-object (condition stream :type t)
           (%print-condition-inside condition stream)))
        (t (%report-condition condition stream)))
  condition)

;;; What SBCL shows inside `#<...>` for a condition: a simple condition's
;;; format control, a type error's expected type and datum. The library's
;;; own conditions show their message.
(defun %print-condition-inside (condition stream)
  (cond ((slot-value condition '%message) (prin1 (slot-value condition '%message) stream))
        ((and (typep condition 'simple-condition)
              (stringp (simple-condition-format-control condition)))
         (prin1 (simple-condition-format-control condition) stream))
        ((and (typep condition 'type-error)
              (slot-boundp condition 'datum) (slot-boundp condition 'expected-type))
         (format stream "expected-type: ~S datum: ~S"
                 (type-error-expected-type condition) (type-error-datum condition)))))

(defun %report-condition (condition stream)
  (let ((message (slot-value condition '%message)))
    (if message
        ;; The message, and what it is about: the library signals
        ;; `(error 'type "there is no class with this name" name)`, and a
        ;; report that stopped at the message never said which class (#188).
        (let ((data (slot-value condition '%data)))
          (write-string message stream)
          (when (consp data)
            (write-string ":" stream)
            (dolist (each data)
              (write-char #\Space stream)
              (prin1 each stream))))
        (let ((report (%condition-report (class-of condition))))
          (cond ((stringp report) (write-string report stream))
                (report (funcall report condition stream))
                (t (format stream "Condition ~A was signalled."
                           (%name-with-prefix (type-of condition) t))))))))

;;; NAME as SBCL's reports print a name (`sb-ext:print-symbol-with-prefix`):
;;; with its package's name in front, `COMMON-LISP-USER::ZZ`, whatever the
;;; current package. With EXCEPT-STANDARD, a standard name is printed bare,
;;; as SBCL's default report prints `Condition ERROR was signalled.`
(defun %name-with-prefix (name &optional except-standard)
  (if (and (symbolp name) (symbol-package name)
           (not (and except-standard (eq (symbol-package name) (find-package "COMMON-LISP")))))
      (let ((*package* (find-package "KEYWORD")))
        (prin1-to-string name))
      (prin1-to-string name)))

;;; The report of the first class in CLASS's precedence list that has one.
(defun %condition-report (class)
  (dolist (each (%class-precedence-list class) nil)
    (let ((report (getf (%class-plist each) '%report)))
      (when report (return report)))))

;;; A condition's message as text, for a person to read: what a host prints
;;; for an error nobody handled (`%host-step`). A report that itself fails
;;; gives SBCL's `#<error printing a BAD>`: the error it signals is reported
;;; by this function too, and would fail again, for ever.
;;;
;;; The report is printed with `*print-level*` and `*print-length*` bound,
;;; as the machine's own printer of errors bounded its depth: a datum can be
;;; a circular list, `(funcall x)` with X its own car, and printed in full
;;; it would never end.
(defun %condition-text (condition)
  (if (%conditionp condition)
      (let ((message (%condition-message condition)))
        (if (stringp message) message (princ-to-string message)))
      (let ((*print-level* 5) (*print-length* 20))
        (handler-case (princ-to-string condition)
          (error () (format nil "#<error printing a ~A>" (%condition-type-of condition)))))))

;;; The standard types' reports, in SBCL's words, as SBCL prints them with
;;; `*print-pretty*` false: with it true SBCL breaks the longer ones over
;;; several lines, and the pretty printer is tier 2.
(%set-condition-report
 (find-class 'simple-condition)
 (lambda (condition stream)
   (let ((control (simple-condition-format-control condition)))
     (if control
         (let ((arguments (simple-condition-format-arguments condition)))
           (apply #'format stream control arguments)
           ;; `(error "there is no class with this name" name)`: a control
           ;; with no directive in it, and arguments. SBCL would print the
           ;; message and drop them; the library writes its errors this way
           ;; throughout, and the arguments are what the message is about
           ;; (#188).
           (when (and arguments (stringp control) (not (find #\~ control)))
             (write-string ":" stream)
             (dolist (each arguments)
               (write-char #\Space stream)
               (prin1 each stream))))
         (format stream "Condition ~S was signalled." (type-of condition))))))

(%set-condition-report
 (find-class 'type-error)
 (lambda (condition stream)
   (format stream "The value ~S is not of type ~S"
           (type-error-datum condition) (type-error-expected-type condition))))

(%set-condition-report
 (find-class 'unbound-variable)
 (lambda (condition stream)
   (format stream "The variable ~S is unbound." (cell-error-name condition))))

(%set-condition-report
 (find-class 'undefined-function)
 (lambda (condition stream)
   (format stream "The function ~A is undefined." (%name-with-prefix (cell-error-name condition)))))

(%set-condition-report
 (find-class 'unbound-slot)
 (lambda (condition stream)
   (format stream "The slot ~A is unbound in the object ~S."
           (%name-with-prefix (cell-error-name condition)) (unbound-slot-instance condition))))

(%set-condition-report
 (find-class 'print-not-readable)
 (lambda (condition stream)
   (format stream "~S cannot be printed readably." (print-not-readable-object condition))))

(%set-condition-report
 (find-class 'arithmetic-error)
 (lambda (condition stream)
   (format stream "arithmetic error ~S signalled" (type-of condition))
   (when (slot-boundp condition 'operation)
     (format stream "~%Operation was ~S."
             (cons (arithmetic-error-operation condition)
                   (arithmetic-error-operands condition))))))

(%set-condition-report
 (find-class 'end-of-file)
 (lambda (condition stream)
   (format stream "end of file on ~S" (stream-error-stream condition))))

;;; ------------------------------------------------------------------------
;;; Signalling (sub-step 2)
;;;
;;; A handler that `handler-bind` establishes runs where the condition is
;;; signalled, on top of the frames that signalled it, and may decline by
;;; returning (CLHS 9.1.4). The handlers are a *dynamic* stack: each
;;; `handler-bind` binds `%handler-clusters`, a special variable, to its
;;; cluster of handlers for the extent of its body, the textbook technique
;;; (SBCL's `*handler-clusters*`), and an unwind takes the binding off with
;;; everything else.
;;;
;;; The prelude's `handler-case` cannot become a macro over `handler-bind`:
;;; the compiler uses it and the fixpoint needs it compiled as it is, so it
;;; keeps its `HANDLER` records. Both are records on the machine's one mark
;;; chain, and `%handler-marks` reads them off it in their true order, so a
;;; `handler-case` and a `handler-bind` nest as CLHS says whichever was
;;; established first (`docs/conditions.md`).

;;; The cluster the innermost `handler-bind` established; the walk reads
;;; every binding of it, not only this value.
(defvar %handler-clusters nil)

;;; While a handler runs, the place of its cluster: the walk hides that
;;; cluster and everything established between it and the signal, so a
;;; handler sees only the handlers outside its own (CLHS 9.1.4.1).
(defvar %handler-floor nil)

;;; CLHS's: a condition of this type, when signalled, enters the debugger
;;; before any handler sees it (`%signal` asks).
(defvar *break-on-signals* nil)

;;; `(handler-bind ((type handler)...) form...)`: each HANDLER is a form
;;; whose value is a function designator, evaluated now, in order.
(defmacro handler-bind (bindings &body forms)
  (dolist (binding bindings)
    (unless (and (consp binding) (consp (cdr binding)) (null (cddr binding)))
      (error 'program-error "a handler-bind binding is (type handler)" binding)))
  `(let ((%handler-clusters
           (list ,@(mapcar (lambda (binding) `(cons ',(car binding) ,(cadr binding))) bindings))))
     ,@forms))

;;; Offer CONDITION to every active handler, innermost first. A
;;; `handler-case` whose clause wants it is unwound to, and that is the end
;;; of the signal; a `handler-bind` handler that wants it is called, with
;;; the floor set, and if it returns, the search goes on: first through the
;;; rest of its own cluster, in order, then outwards. Returns nil when no
;;; handler took a non-local exit.
;;;
;;; The floor is set for the type tests too, as SBCL's is: `typep` may
;;; itself signal, for a type nobody defined or a `satisfies` predicate
;;; that fails, and that error must go outwards. Searched from the top
;;; again it would meet the same test and fail again, for ever.
(defun %signal (condition)
  (%break-on-signal condition)
  (dolist (entry (%handler-marks '%handler-clusters '%handler-floor) nil)
    (let ((%handler-floor (cadr entry)))
      (if (eql (car entry) 0)
          (dolist (clause (cddr entry))
            (when (typep condition (car clause))
              (%unwind-to-handler (cadr entry) condition)))
          (dolist (handler (cddr entry))
            (when (typep condition (car handler))
              (funcall (cdr handler) condition)))))))

;;; The condition DATUM and ARGUMENTS designate (CLHS 9.1.2.1): a condition
;;; as it is; a condition type and its initargs; a format control and its
;;; arguments, as a condition of DEFAULT-TYPE. And the library's own form, a
;;; type then a message and data, whose condition reports the message.
(defun %designated-condition (datum arguments default-type)
  (cond ((%condition-object-p datum)
         (when arguments
           (error 'type-error "a condition was given with arguments" arguments))
         datum)
        ((and (symbolp datum) arguments (stringp (car arguments)))
         (let ((condition (make-condition datum)))
           (setf (slot-value condition '%message) (car arguments)
                 (slot-value condition '%data) (cdr arguments))
           condition))
        ((symbolp datum) (apply #'make-condition datum arguments))
        ((or (stringp datum) (functionp datum))
         (make-condition default-type :format-control datum :format-arguments arguments))
        (t (error 'type-error "a condition designator is a condition, a symbol or a format control"
                  datum))))

;;; `(signal datum &rest arguments)`: offer the condition to the handlers,
;;; and return nil if none took it.
(defun signal (datum &rest arguments)
  (%signal (%designated-condition datum arguments 'simple-condition))
  nil)

;;; `(error datum &rest arguments)`: signal, and if no handler takes the
;;; condition, enter the debugger (CLHS), which does not return.
(defun error (datum &rest arguments)
  (let ((condition (%designated-condition datum arguments 'simple-error)))
    (%signal condition)
    (invoke-debugger condition)))

;;; ------------------------------------------------------------------------
;;; Entering the debugger (sub-step 4)
;;;
;;; The debugger itself is `debugger.lisp`, and is off in a new world. With
;;; it off, entering it means what CLHS says before any debugger runs: `*debugger-hook*`, if a program
;;; set one, is called with the condition and itself, bound to nil while it
;;; runs. If it returns, or there is none, the innermost `abort` restart is
;;; invoked, with the condition left in `%aborted-condition` for whoever
;;; established the restart to report (the host loop, `%host-step`); and
;;; with no `abort` restart either, the machine halts with the condition, as
;;; an error nobody handled always has, and a host reports it.

(defvar *debugger-hook* nil)

;;; The condition the debugger gave up on, for the host loop's `abort`
;;; restart, `%host-abort-restart`, which `%host-step` binds, as the place
;;; to say why. A program's own `abort` restart is told nothing: the
;;; condition would be left behind for a later `(abort)` of the host's.
(defvar %aborted-condition nil)
(defvar %host-abort-restart nil)

;;; Whether `invoke-debugger` talks to the person at the console (step F,
;;; #131, `debugger.lisp`), or does what it did before there was a debugger:
;;; invoke the innermost `abort` restart, so that a host reports the error
;;; and carries on. Off in a new world, since a world also runs where nobody
;;; is at a console, in tests, scripts and the browser page; the terminal
;;; host turns it on when its input is a terminal.
(defvar %debugger-enabled nil)

(defun invoke-debugger (condition)
  (let ((hook *debugger-hook*))
    (when hook
      (let ((*debugger-hook* nil))
        (funcall hook condition hook))))
  (if %debugger-enabled
      (%debugger condition)
      (%without-a-debugger condition)))

(defun %without-a-debugger (condition)
  (let ((abort (find-restart 'abort condition)))
    (when abort
      (if (eq abort %host-abort-restart) (setq %aborted-condition condition))
      (invoke-restart abort))
    (%signalled (%condition-type-of condition) (%condition-text condition))))

;;; ------------------------------------------------------------------------
;;; The machine's own errors (sub-step 3)
;;;
;;; When a primitive or an instruction fails, `(car 5)` say, the machine
;;; calls this function where the error happened, with the condition's
;;; type, its own message, and a property list of what it knows: `:name`
;;; for an unbound variable or undefined function, `:datum` and
;;; `:expected-type` for a value called that is not a function or an index
;;; out of bounds, `:operation` and `:operands` for a primitive that failed
;;; (`Vm::fail_with`). Here the condition is made with the initargs CLHS
;;; gives its type, and signalled like any other, so a `handler-bind` sees
;;; it with the failing frames still on the stack.

;;; How deep the machine's errors are nested *while their conditions are
;;; being made*: an error in making one, which a bug could make happen
;;; every time, stops the machine on the fourth instead of going round for
;;; ever. Only the making counts. The handlers run outside the binding,
;;; since a handler that meets a machine error of its own is an ordinary
;;; program, however deeply it nests (the review found the first version
;;; halting one).
(defvar %machine-error-depth 0)

(defun %signal-machine-error (type message context)
  (let ((depth (if (integerp %machine-error-depth) %machine-error-depth 0)))
    (if (> depth 3)
        (%signalled type message)
        (error (let ((%machine-error-depth (+ depth 1)))
                 (%machine-condition type message context))))))

;;; The condition: of TYPE, with the initargs the context gives, and the
;;; machine's message as its report when it gives none.
(defun %machine-condition (type message context)
  (let ((class (find-class type nil)))
    (if (%condition-class-p class)
        (let* ((initargs (%machine-initargs class context))
               (condition (apply #'make-condition class initargs)))
          (unless initargs
            (setf (slot-value condition '%message) message))
          condition)
        (make-condition 'simple-error :format-control "~A" :format-arguments (list message)))))

(defun %machine-initargs (class context)
  (let ((precedence (%class-precedence-list class))
        (operation (getf context :operation))
        (operands (getf context :operands)))
    (cond ((%member-eq (find-class 'cell-error) precedence)
           (cond ((%plist-has-key context :name) (list :name (getf context :name)))
                 (operation (list :name (first operands)))
                 (t nil)))
          ((%member-eq (find-class 'type-error) precedence)
           (if (%plist-has-key context :datum)
               (list :datum (getf context :datum) :expected-type (getf context :expected-type))
               (let ((culprit (%primitive-type-culprit operation operands)))
                 (if culprit (list :datum (car culprit) :expected-type (cdr culprit)) nil))))
          ((and operation (%member-eq (find-class 'arithmetic-error) precedence))
           (list :operation operation :operands operands))
          (t nil))))

;;; The types a primitive takes, by argument: what `(car 5)` was given that
;;; it should not have been, and what it wanted instead. The Rust says only
;;; "CAR wants a list", in words; the types are Lisp's to know, as SBCL
;;; knows them from its functions' declared types. A primitive missing
;;; here signals its `type-error` without a datum, reporting the machine's
;;; message.
(defvar %primitive-argument-types
  '((car list) (cdr list) (rplaca cons) (rplacd cons)
    (symbol-name symbol) (symbol-function symbol) (symbol-value symbol)
    (symbol-plist symbol) (boundp symbol) (makunbound symbol) (set symbol)
    (make-symbol string)
    (char-code character) (char-upcase character) (char-downcase character)
    (alpha-char-p character) (code-char integer)
    (+ &rest number) (- &rest number) (* &rest number) (/ &rest number) (= &rest number)
    (< &rest real) (> &rest real) (rem real real)
    (values-list list) (hash-table-count hash-table) (hash-table-test hash-table)
    (remhash t hash-table) (%gethash t hash-table t) (%sequence-length sequence)
    (aref array &rest integer)
    (%fixnum-logand integer integer) (%fixnum-logior integer integer)
    (%fixnum-logxor integer integer)))

;;; The first of OPERANDS that is not of the type OPERATION takes there, and
;;; that type, as (datum . type); nil if none can be named.
(defun %primitive-type-culprit (operation operands)
  (let ((types (dolist (entry %primitive-argument-types nil)
                 (when (eq (car entry) operation) (return (cdr entry)))))
        (every-other nil))
    (when types
      (dolist (operand operands nil)
        (when (eq (car types) '&rest)
          (setq every-other (cadr types) types nil))
        (let ((type (or every-other (car types))))
          (when (and type (not (typep operand type)))
            (return (cons operand type)))
          (unless every-other (setq types (cdr types))))))))

;;; ------------------------------------------------------------------------
;;; Restarts (sub-step 4)
;;;
;;; A restart is a way out that the code near an error offers and the code
;;; far from it chooses (CLHS 9.1.4.2): `restart-case` around a computation
;;; names its ways out, a handler further up finds one with `find-restart`
;;; and takes it with `invoke-restart`. Restarts are a dynamic stack, kept
;;; as handlers are, in a special variable, `%restart-clusters`, bound by
;;; `restart-bind` to its cluster consed onto the ones outside it. Unlike
;;; handlers they never meet the prelude's `handler-case` records, so the
;;; binding is all there is to them.

;;; A restart: a structure, as SBCL's is. CLHS names only `restart-name`.
(defstruct (restart (:constructor %make-restart) (:conc-name %restart-)
                    (:predicate %restartp) (:copier nil))
  name function report-function interactive-function test-function
  (conditions nil))

(defun restart-name (restart) (%restart-name restart))

;;; Escaped, `#<RESTART ABORT>`; otherwise its report, which is what a
;;; debugger shows a person choosing among restarts.
(defmethod print-object ((restart restart) stream)
  (if *print-escape*
      (print-unreadable-object (restart stream :type t)
        (prin1 (%restart-name restart) stream))
      (let ((report (%restart-report-function restart)))
        (if report
            (funcall report stream)
            (prin1 (%restart-name restart) stream))))
  restart)

(defvar %restart-clusters nil)

;;; `(restart-bind ((name function [:report-function f] [:interactive-function f]
;;; [:test-function f])...) form...)`
(defmacro restart-bind (bindings &body forms)
  `(let ((%restart-clusters
           (cons (list ,@(mapcar (lambda (binding)
                                   `(%make-restart :name ',(car binding)
                                                   :function ,(cadr binding)
                                                   ,@(cddr binding)))
                                 bindings))
                 %restart-clusters)))
     ,@forms))

;;; The active restarts, innermost first. Given a condition, a restart that
;;; `with-condition-restarts` associated with another condition is left
;;; out (CLHS 9.1.4.2.2), and so is one whose test refuses the condition.
(defun compute-restarts (&optional condition)
  (unless (or (null condition) (%condition-object-p condition))
    (error 'type-error :datum condition :expected-type '(or condition null)))
  (let ((found nil))
    (dolist (cluster %restart-clusters (nreverse found))
      (dolist (restart cluster)
        (when (%restart-visible-p restart condition)
          (push restart found))))))

(defun %restart-visible-p (restart condition)
  (and (or (null condition)
           (null (%restart-conditions restart))
           (%member-eq condition (%restart-conditions restart)))
       (let ((test (%restart-test-function restart)))
         (or (null test) (funcall test condition)))))

;;; The innermost active restart named IDENTIFIER, or IDENTIFIER itself if
;;; it is an active restart.
(defun find-restart (identifier &optional condition)
  (unless (or (%restartp identifier) (and identifier (symbolp identifier)))
    (error 'type-error :datum identifier :expected-type '(or restart (and symbol (not null)))))
  (dolist (restart (compute-restarts condition) nil)
    (when (if (%restartp identifier)
              (eq restart identifier)
              (eq (%restart-name restart) identifier))
      (return restart))))

;;; The restart IDENTIFIER designates, if it is active. A restart object is
;;; active while its `restart-bind` is: its test is not asked again, since
;;; the code that found it, with the condition, has asked it already
;;; (SBCL; the review found `(invoke-restart (find-restart 'foo c))`
;;; failing for a restart with a :test).
(defun %active-restart (identifier condition)
  (or (if (%restartp identifier)
          (dolist (cluster %restart-clusters nil)
            (when (%member-eq identifier cluster) (return identifier)))
          (find-restart identifier condition))
      (error 'control-error "there is no active restart of that name" identifier)))

(defun invoke-restart (restart &rest arguments)
  (apply (%restart-function (%active-restart restart nil)) arguments))

;;; As `invoke-restart`, with the arguments the restart's interactive
;;; function asks a person for, or none.
(defun invoke-restart-interactively (restart)
  (let* ((found (%active-restart restart nil))
         (interactive (%restart-interactive-function found)))
    (apply (%restart-function found) (if interactive (funcall interactive) nil))))

;;; Associate each of RESTARTS with CONDITION for the extent of the body.
(defmacro with-condition-restarts (condition-form restarts-form &body forms)
  (let ((condition (gensym "CONDITION")) (restarts (gensym "RESTARTS")))
    `(let ((,condition ,condition-form) (,restarts ,restarts-form))
       (dolist (restart ,restarts)
         (setf (%restart-conditions restart) (cons ,condition (%restart-conditions restart))))
       (unwind-protect (progn ,@forms)
         (dolist (restart ,restarts)
           (setf (%restart-conditions restart)
                 (remove ,condition (%restart-conditions restart) :count 1)))))))

;;; `(restart-case form (name lambda-list [:report r] [:interactive i]
;;; [:test t] body...)...)`, as CLHS describes it: each clause a restart
;;; whose function stores its arguments and leaves by `go` to the clause's
;;; tag, where its body runs with them, outside the restart's extent. When
;;; FORM is a call of `signal`, `error`, `cerror` or `warn`, the condition
;;; is made first and the restarts are associated with it.
(defmacro restart-case (restartable-form &rest clauses &environment environment)
  (let* ((block (gensym "RESTART-CASE"))
         (arguments (gensym "ARGUMENTS"))
         (parsed (mapcar #'%parse-restart-clause clauses)))
    `(block ,block
       (let ((,arguments nil))
         (tagbody
            (restart-bind
                ,(mapcar (lambda (clause)
                           (let ((temp (gensym "ARGUMENTS")))
                             `(,(second clause)
                               (lambda (&rest ,temp) (setq ,arguments ,temp) (go ,(first clause)))
                               ,@(%restart-clause-options clause))))
                         parsed)
              (return-from ,block
                ,(%restartable-form restartable-form environment)))
            ,@(mapcan (lambda (clause)
                        (list (first clause)
                              `(return-from ,block
                                 (apply (lambda ,(third clause) ,@(seventh clause)) ,arguments))))
                      parsed))))))

;;; A clause as (tag name lambda-list report interactive test body).
(defun %parse-restart-clause (clause)
  (unless (and (consp clause) (symbolp (car clause)) (consp (cdr clause)) (listp (cadr clause)))
    (error 'program-error "a restart-case clause is (name lambda-list ...)" clause))
  (let ((rest (cddr clause)) (report nil) (interactive nil) (test nil))
    (do () ((not (and (consp rest) (member (car rest) '(:report :interactive :test)) (consp (cdr rest)))))
      (case (car rest)
        (:report (setq report (cadr rest)))
        (:interactive (setq interactive (cadr rest)))
        (:test (setq test (cadr rest))))
      (setq rest (cddr rest)))
    (list (gensym "RESTART") (car clause) (cadr clause) report interactive test rest)))

(defun %restart-clause-options (clause)
  (let ((report (fourth clause)) (interactive (fifth clause)) (test (sixth clause)))
    (append (cond ((null report) nil)
                  ((stringp report)
                   (let ((stream (gensym "STREAM")))
                     `(:report-function (lambda (,stream) (write-string ,report ,stream)))))
                  (t `(:report-function (function ,report))))
            (if interactive `(:interactive-function (function ,interactive)) nil)
            (if test `(:test-function (function ,test)) nil))))

(defun %restartable-form (form environment)
  (let ((expanded (macroexpand form environment)))
    (if (and (consp expanded) (member (car expanded) '(signal error cerror warn)))
        (let* ((operator (car expanded))
               (temps (mapcar (lambda (argument) argument (gensym "ARGUMENT")) (cdr expanded)))
               (condition (gensym "CONDITION")))
          `(let ,(mapcar #'list temps (cdr expanded))
             (let ((,condition
                     ,(if (eq operator 'cerror)
                          `(%designated-condition ,(second temps) (list ,@(cddr temps)) 'simple-error)
                          `(%designated-condition ,(first temps) (list ,@(rest temps))
                                                  ',(case operator
                                                      (signal 'simple-condition)
                                                      (warn 'simple-warning)
                                                      (t 'simple-error))))))
               (with-condition-restarts ,condition (car %restart-clusters)
                 ,(if (eq operator 'cerror)
                      `(cerror ,(first temps) ,condition)
                      `(,operator ,condition))))))
        form)))

;;; `(with-simple-restart (name format-control argument...) form...)`
(defmacro with-simple-restart ((name format-control &rest format-arguments) &body forms)
  (let ((stream (gensym "STREAM")))
    `(restart-case (progn ,@forms)
       (,name ()
         :report (lambda (,stream) (format ,stream ,format-control ,@format-arguments))
         (values nil t)))))

;;; The standard restarts' functions (CLHS 9.2). `abort` and
;;; `muffle-warning` must find their restart; the others do nothing when
;;; there is none.
;;; `abort` and `muffle-warning` do not return: a restart of theirs that
;;; does is a `control-error`, as SBCL's are.
(defun abort (&optional condition)
  (invoke-restart (%active-restart 'abort condition))
  (error 'control-error "the ABORT restart returned"))

(defun muffle-warning (&optional condition)
  (invoke-restart (%active-restart 'muffle-warning condition))
  (error 'control-error "the MUFFLE-WARNING restart returned"))

(defun continue (&optional condition)
  (let ((restart (find-restart 'continue condition)))
    (if restart (invoke-restart restart) nil)))

(defun store-value (value &optional condition)
  (let ((restart (find-restart 'store-value condition)))
    (if restart (invoke-restart restart value) nil)))

(defun use-value (value &optional condition)
  (let ((restart (find-restart 'use-value condition)))
    (if restart (invoke-restart restart value) nil)))

;;; ------------------------------------------------------------------------
;;; The operators that offer restarts

;;; `(warn datum &rest arguments)`: signal a warning with a `muffle-warning`
;;; restart, and if nobody muffles it, print it on `*error-output*`.
(defun warn (datum &rest arguments)
  (let ((condition (%designated-condition datum arguments 'simple-warning)))
    (unless (typep condition 'warning)
      (error 'type-error :datum condition :expected-type 'warning))
    (restart-case (signal condition)
      (muffle-warning () :report "Skip warning." (return-from warn nil)))
    (format *error-output* "~&WARNING: ~A~%" condition)
    nil))

;;; `(cerror continue-format-control datum &rest arguments)`: an error with
;;; a `continue` restart, which returns nil.
(defun cerror (continue-format-control datum &rest arguments)
  (let ((condition (%designated-condition datum arguments 'simple-error)))
    (restart-case (error condition)
      (continue ()
        :report (lambda (stream) (apply #'format stream continue-format-control arguments))
        nil))))

;;; `(break &optional format-control &rest arguments)`: enter the debugger,
;;; with a `continue` restart, not consulting `*debugger-hook*` (CLHS).
(defun break (&optional (format-control "Break") &rest arguments)
  (with-simple-restart (continue "Return from BREAK.")
    (let ((*debugger-hook* nil))
      (invoke-debugger (make-condition 'simple-condition :format-control format-control
                                                         :format-arguments arguments))))
  nil)

;;; Called by `signal` first: if the condition is of `*break-on-signals*`'s
;;; type, break. The variable is nil while its type is tested, so a type
;;; that cannot be tested signals once and not for ever.
(defun %break-on-signal (condition)
  (let ((type *break-on-signals*))
    (when type
      (let ((*break-on-signals* nil))
        (when (typep condition type)
          (break "~A~%BREAK was entered because of *BREAK-ON-SIGNALS*." condition))))))

;;; A new value for PLACE, from a person: what `store-value` and `assert`'s
;;; `continue` ask for when invoked interactively.
(defun %prompt-for-value (place)
  (format *query-io* "~&Enter a form for a new value of ~S: " place)
  (list (eval (read *query-io*))))

;;; `(assert test-form [(place...) [datum argument...]])`: an error until
;;; TEST-FORM is true, with a `continue` restart that tries again, asking
;;; for new values of the places first.
(defmacro assert (test-form &optional places datum &rest arguments)
  (let ((again (gensym "AGAIN")) (stream (gensym "STREAM")))
    `(tagbody
        ,again
        (unless ,test-form
          (restart-case ,(if datum
                             `(error ,datum ,@arguments)
                             `(error 'simple-error :format-control "The assertion ~S failed."
                                                   :format-arguments (list ',test-form)))
            (continue ()
              :report (lambda (,stream)
                        ,(if places
                             `(format ,stream "Retry assertion with new value~P for ~{~S~^, ~}."
                                      ,(length places) ',places)
                             `(write-string "Retry assertion." ,stream)))
              ;; As SBCL's: a new value for each place is asked for when
              ;; the restart is taken, however it is invoked.
              ,@(mapcar (lambda (place) `(setf ,place (car (%prompt-for-value ',place))))
                        places)
              (go ,again)))))))

;;; `(check-type place type [type-string])`: a `type-error` until PLACE's
;;; value is of TYPE, with a `store-value` restart that stores a new value
;;; in the place and checks again. Moved here from `types.lisp`, where it
;;; had no restart to offer.
(defmacro check-type (place type &optional type-string)
  (let ((again (gensym "AGAIN")) (value (gensym "VALUE")) (new (gensym "NEW"))
        (stream (gensym "STREAM")))
    `(tagbody
        ,again
        (let ((,value ,place))
          (unless (typep ,value ',type)
            (restart-case (error 'simple-type-error :datum ,value :expected-type ',type
                                 :format-control "The value of ~S is ~S, which is not ~A."
                                 :format-arguments (list ',place ,value
                                                         ,(or type-string
                                                              (format nil "of type ~S" type))))
              (store-value (,new)
                :report (lambda (,stream) (format ,stream "Supply a new value for ~S." ',place))
                :interactive (lambda () (%prompt-for-value ',place))
                (setf ,place ,new)
                (go ,again))))))))

;;; `ccase` and `ctypecase`: as `ecase` and `etypecase`, with a
;;; `store-value` restart that stores a new key in the place and tries
;;; again. Moved here from `lists.lisp` and `types.lisp`.
(defmacro ccase (keyplace &body clauses)
  (%correctable-case keyplace
                     (lambda (key) (%case-clauses key (%literal-keys clauses) nil))
                     (cons 'member (%ecase-keys clauses))))

(defmacro ctypecase (keyplace &body clauses)
  (%correctable-case keyplace
                     (lambda (key) (%etypecase-clauses key clauses))
                     (cons 'or (mapcar #'car clauses))))

(defun %correctable-case (keyplace make-clauses expected-type)
  (let ((again (gensym "AGAIN")) (block (gensym "CASE")) (key (gensym "KEY"))
        (new (gensym "NEW")) (stream (gensym "STREAM")))
    `(block ,block
       (tagbody
          ,again
          (let ((,key ,keyplace))
            (return-from ,block
              (cond ,@(funcall make-clauses key)
                    (t (restart-case (error 'type-error :datum ,key :expected-type ',expected-type)
                         (store-value (,new)
                           :report (lambda (,stream) (format ,stream "Supply a new value for ~S." ',keyplace))
                           :interactive (lambda () (%prompt-for-value ',keyplace))
                           (setf ,keyplace ,new)
                           (go ,again)))))))))))
