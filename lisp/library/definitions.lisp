;;;; definitions.lisp — the world's definitions keep their source (phase 3,
;;;; step C, #55; the design is `docs/definitions.md`)
;;;;
;;;; A compiled function is bytecode and constants, and until this step the
;;;; form it was compiled from was dropped as soon as it was compiled. Now a
;;;; definition evaluated at the top level, by `load`, by the host's
;;;; read-eval-print loop or by `eval`, is kept as an object: its kind and
;;;; name, its form, the text it was written as, with the comments above it,
;;;; and the file and position it came from. Redefining it keeps the old
;;;; definition as a version. This is the first layer of what
;;;; `docs/design.md` calls the world as a database of definitions (#126),
;;;; which the debugger (phase 3, step F) and the environment (phases 4 to 6)
;;;; are built on.
;;;;
;;;; The system's own definitions are not recorded: the cold build loads
;;;; the library with the Rust reader, and the warm rebuild turns recording
;;;; off (`%recording-definitions`).

(defclass definition ()
  ((kind :initarg :kind :reader definition-kind)
   (name :initarg :name :reader definition-name)
   (form :initarg :form :reader definition-form)
   (text :initarg :text :initform nil :reader definition-text)
   (source :initarg :source :initform nil :reader definition-source)
   (position :initarg :position :initform nil :reader definition-position)
   (package :initarg :package :initform nil :reader definition-package)
   (object :initarg :object :initform nil :reader definition-object)
   (versions :initarg :versions :initform nil :accessor definition-versions)))

;;; `#<DEFINITION :FUNCTION FOO>`.
(defmethod print-object ((definition definition) stream)
  (print-unreadable-object (definition stream :type t)
    (format stream "~S ~S" (definition-kind definition) (definition-name definition))))

;;; Every definition the world has recorded, keyed by (kind . name).
(defvar %definitions (make-hash-table :test 'equal))

;;; Whether evaluating a defining form records it: false while the world
;;; rebuilds itself (`rebuild.lisp`).
(defvar %recording-definitions t)

;;; Where the top-level form being evaluated was read from, which `load` and
;;; the host loop know and `eval` does not: they bind these around each form,
;;; and `eval` takes them for that form alone. The place is (string start
;;; end limit), positions in the string read from, and the form's text is
;;; cut from it only if the form turns out to define something, since
;;; cutting costs more than many a form does to evaluate.
(defvar %definition-place nil)
(defvar %definition-source nil)

;;; The defining operators the world records, and the kind each defines.
;;; A program's own `def...` macro is not among them: there is no telling
;;; from its name what, if anything, it defines.
(defvar %definition-kinds
  '((defun . :function) (defmacro . :macro) (defgeneric . :generic-function)
    (defmethod . :method) (defvar . :variable) (defparameter . :variable)
    (defconstant . :constant) (defclass . :class) (defstruct . :structure)
    (define-condition . :condition) (deftype . :type) (defsetf . :setf-expander)
    (define-setf-expander . :setf-expander) (define-modify-macro . :macro)
    (define-compiler-macro . :compiler-macro) (define-symbol-macro . :symbol-macro)
    (define-method-combination . :method-combination) (defpackage . :package)))

;;; The kind and name FORM defines, as two values, or nil. A method's name
;;; is its generic function's name, its qualifiers and its specializers, as
;;; `find-method` tells methods apart.
(defun %definition-key (form)
  (let ((kind (and (consp form) (symbolp (car form))
                   (cdr (assoc (car form) %definition-kinds)))))
    (if (and kind (consp (cdr form)))
        (let ((name (second form)))
          (case kind
            (:structure (values kind (if (consp name) (car name) name)))
            (:method (values kind (%method-definition-name form)))
            ;; A package is named by its name's string: `#:pk`, `:pk` and
            ;; "PK" are one package.
            (:package (values kind (if (or (stringp name) (symbolp name) (characterp name))
                                       (string name)
                                       name)))
            (t (values kind name))))
        nil)))

(defun %method-definition-name (form)
  (let ((qualifiers nil) (rest (cddr form)))
    (do () ((or (null rest) (listp (car rest))))
      (push (car rest) qualifiers)
      (setq rest (cdr rest)))
    (list (second form) (nreverse qualifiers) (extract-specializer-names (car rest)))))

;;; The definition of KIND named NAME, or nil.
(defun find-definition (name &optional (kind :function))
  (gethash (cons kind name) %definitions))

;;; Every definition named NAME, of any kind.
(defun definitions-named (name)
  (let ((found nil))
    (maphash (lambda (key definition) (when (equal (cdr key) name) (push definition found)))
             %definitions)
    found))

;;; Record FORM, just evaluated, if it is a defining form, with the text
;;; cut from PLACE if it came from one; the forms of a `progn` or an
;;; `eval-when` that ran are each recorded, without text of their own (the
;;; text of the whole is not any one of theirs).
(defun %record-definition (form place source)
  (when %recording-definitions
    (cond ((and (consp form) (eq (car form) 'progn))
           (dolist (sub (cdr form)) (%record-definition sub nil source)))
          ((and (consp form) (eq (car form) 'eval-when)
                (or (member :execute (cadr form)) (member 'eval (cadr form))))
           (dolist (sub (cddr form)) (%record-definition sub nil source)))
          (t (multiple-value-bind (kind name) (%definition-key form)
               (when kind
                 (multiple-value-bind (text position) (%place-text place)
                 (let* ((key (cons kind name))
                        (old (gethash key %definitions))
                        (new (make-instance 'definition
                                            :kind kind :name name :form form :text text
                                            :source source :position position
                                            :package *package*
                                            :object (%definition-object kind name)
                                            :versions (if old
                                                          (cons old (definition-versions old))
                                                          nil))))
                   ;; The old definition is a version now; its own versions
                   ;; are the new one's, so the history is one list.
                   (when old (setf (definition-versions old) nil))
                   (setf (gethash key %definitions) new)))))))))

;;; What a definition made, where there is one object to point at.
(defun %definition-object (kind name)
  (case kind
    ((:function :generic-function) (and (fboundp name) (fdefinition name)))
    ;; The macro object itself: `macro-function` makes a new wrapper of it
    ;; each time it is asked, which nothing could compare.
    (:macro (and (symbolp name) (%macro-function name)))
    ((:class :structure :condition) (and (symbolp name) (find-class name nil)))
    (:package (find-package name))
    (t nil)))

;;; The characters a blank line may hold besides its newline: a file with
;;; CRLF line ends has a return before each.
(defvar %blank-characters '(#\Space #\Tab #\Return))

;;; The text of the form PLACE says was read, and where it begins, as two
;;; values; nil for no place. #126 decided which comments belong to a
;;; definition, so that writing it back reproduces the file:
;;;
;;; 1. The line comments directly above it, with no blank line between,
;;;    are its own. Comments further up, a file's opening ones say, belong
;;;    to what comes before or to the package.
;;; 2. A comment at the end of its last line is its own, and is not the next
;;;    definition's: `(defvar *n* 0)   ; how many` keeps its note.
(defun %place-text (place)
  (if place
      (destructuring-bind (string start end limit) place
        (let ((text-start (min (%own-comments-start string start limit) end))
              (text-end (%trailing-comment-end string end limit)))
          (values (string-right-trim (cons #\Newline %blank-characters)
                                     (subseq string text-start text-end))
                  text-start)))
      (values nil nil)))

;;; Past a comment that ends the line END is on, if there is one, and not
;;; past LIMIT, the end of what the stream may read.
(defun %trailing-comment-end (string end limit)
  (let ((i end))
    (do () ((or (>= i limit) (not (member (char string i) '(#\Space #\Tab)))))
      (setq i (+ i 1)))
    (if (and (< i limit) (char= (char string i) #\;))
        (or (position #\Newline string :start i :end limit) limit)
        end)))

;;; Where a definition's text begins, at or after START.
(defun %own-comments-start (string start limit)
  (let ((i start) (own nil))
    ;; START in the middle of a line is the end of the previous form: a
    ;; comment ending that line is the previous definition's. Another form
    ;; may follow on the same line, and then that is where this one starts.
    (when (and (> i 0) (< i limit) (not (char= (char string (- i 1)) #\Newline)))
      (let ((next (or (position-if-not (lambda (c) (member c '(#\Space #\Tab)))
                                       string :start i :end limit)
                      limit)))
        (if (and (< next limit) (char= (char string next) #\;))
            (setq i (let ((newline (position #\Newline string :start next :end limit)))
                      (if newline (+ newline 1) limit)))
            (setq i next))))
    ;; Line by line: a blank line forgets the comments above it; the first
    ;; line that is neither blank nor a comment is where the form begins.
    (do () ((>= i limit) (or own i))
      (let* ((line-end (or (position #\Newline string :start i :end limit) limit))
             (first (position-if-not (lambda (c) (member c %blank-characters)) string
                                     :start i :end line-end)))
        (cond ((null first) (setq own nil))
              ((char= (char string first) #\;) (unless own (setq own i)))
              (t (return (or own first))))
        (setq i (+ line-end 1))))))

(defun %stream-position (stream)
  (if (%string-input-stream-p stream) (%string-input-stream-position stream) nil))

;;; Evaluate FORM, read from STREAM for `load` or the host loop from START
;;; to where the stream is now, with that place known: the one place a
;;; top-level form's text can be found. A stream that is not a string's
;;; gives no place.
(defun %eval-read-form (form stream start source)
  (let* ((end (%stream-position stream))
         (%definition-place
           (if (and start end)
               (let ((string (%string-input-stream-string stream)))
                 (list string start end (or (%string-input-stream-end stream) (length string))))
               nil))
         (%definition-source source))
    (eval form)))
