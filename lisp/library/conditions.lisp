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
;;;; Signalling is the next sub-step. Until then the only conditions
;;;; signalled are still the machine's, which `class-of` maps to their
;;;; classes by name, so `typep` and `handler-case` treat both alike; the
;;;; readers do not work on one of those, which has no slots.

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
        (write-string message stream)
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
;;; for an error nobody handled (`%host-step`).
(defun %condition-text (condition)
  (if (%conditionp condition)
      (let ((message (%condition-message condition)))
        (if (stringp message) message (princ-to-string message)))
      (princ-to-string condition)))

;;; The standard types' reports, in SBCL's words, as SBCL prints them with
;;; `*print-pretty*` false: with it true SBCL breaks the longer ones over
;;; several lines, and the pretty printer is tier 2.
(%set-condition-report
 (find-class 'simple-condition)
 (lambda (condition stream)
   (let ((control (simple-condition-format-control condition)))
     (if control
         (apply #'format stream control (simple-condition-format-arguments condition))
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
(defun %signal (condition)
  (dolist (entry (%handler-marks '%handler-clusters '%handler-floor) nil)
    (let ((place (cadr entry)))
      (if (eql (car entry) 0)
          (dolist (clause (cddr entry))
            (when (typep condition (car clause))
              (%unwind-to-handler place condition)))
          (dolist (handler (cddr entry))
            (when (typep condition (car handler))
              (let ((%handler-floor place))
                (funcall (cdr handler) condition))))))))

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
;;; condition, stop. CLHS has `error` call `invoke-debugger` then; until the
;;; debugger exists (step F) the machine halts with the condition, as it
;;; always has, and a host reports it.
(defun error (datum &rest arguments)
  (let ((condition (%designated-condition datum arguments 'simple-error)))
    (%signal condition)
    (%signalled (%condition-type-of condition) (%condition-text condition))))
