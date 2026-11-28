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
   (versions :initarg :versions :initform nil :accessor definition-versions)
   ;; What compiling it named: the global functions it calls and the
   ;; macros it expanded (sub-step 2), for `who-calls` and staleness.
   (calls :initarg :calls :initform nil :reader definition-calls)
   (macros :initarg :macros :initform nil :reader definition-macros)
   ;; True when a macro it expanded, or a name it called that is now a
   ;; macro, has been defined since it was compiled: its code is older than
   ;; what its form now means.
   (stale :initarg :stale :initform nil :accessor definition-stale)
   ;; The order definitions were made in, which recompiling follows.
   (number :initarg :number :initform 0 :reader definition-number)))

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

;;; How many definitions the world has made, for their order.
(defvar %definition-count 0)

;;; Record FORM, just evaluated, if it is a defining form, with the text
;;; cut from PLACE if it came from one, and REFERENCES, the compiler's
;;; (macros . calls), if they were collected; the forms of a `progn` or an
;;; `eval-when` that ran are each recorded, without text or references of
;;; their own (those of the whole are not any one of theirs).
(defun %record-definition (form place source &optional references)
  (when %recording-definitions
    ;; The definitions inside a `progn` or an `eval-when` are recorded as
    ;; each is evaluated, with its references (`%eval-part` in `eval.lisp`).
    (cond (t (multiple-value-bind (kind name) (%definition-key form)
               (when kind
                 (multiple-value-bind (text position) (%place-text place)
                 (let* ((key (cons kind name))
                        (old (gethash key %definitions))
                        (new (make-instance 'definition
                                            :kind kind :name name :form form :text text
                                            :source source :position position
                                            :package *package*
                                            :object (%definition-object kind name)
                                            :macros (reverse (car references))
                                            :calls (reverse (cdr references))
                                            :number (setq %definition-count
                                                          (+ %definition-count 1))
                                            ;; A form evaluated again as
                                            ;; it was (a file loaded twice,
                                            ;; a recompile) is no new
                                            ;; version: kept as one, each
                                            ;; reload of a small `defun`
                                            ;; added 5 KB to the world, for
                                            ;; good (#185).
                                            :versions (cond ((null old) nil)
                                                            ((%same-source-p old form text)
                                                             (definition-versions old))
                                                            (t (cons old (definition-versions old)))))))
                   ;; The old definition is a version now; its own versions
                   ;; are the new one's, so the history is one list.
                   (when old (setf (definition-versions old) nil))
                   (setf (gethash key %definitions) new)
                   (when (eq kind :macro) (%mark-stale name))))))))))

;;; Would evaluating FORM, read from TEXT, make what DEFINITION already
;;; is? The same source, and nothing it was compiled with has changed
;;; since: a stale definition compiled again means something else now, and
;;; what it was is worth keeping as a version.
(defun %same-source-p (definition form text)
  (and (not (definition-stale definition))
       (equal (definition-form definition) form)
       (equal (definition-text definition) text)))

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
  (cond ((typep place 'definition)
         ;; Recompiling: the definition's own text, as it was read.
         (values (definition-text place) (definition-position place)))
        (place (%place-text-cut place))
        (t (values nil nil))))

(defun %place-text-cut (place)
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
           ;; Not the console's: its string is only the last line, and a
           ;; form read across lines began in a string that is gone.
           (if (and start end (not (eq stream %console-input)))
               (let ((string (%string-input-stream-string stream)))
                 (list string start end (or (%string-input-stream-end stream) (length string))))
               nil))
         (%definition-source source))
    (eval form)))

;;; ------------------------------------------------------------------------
;;; Cross-reference and staleness (sub-step 2)
;;;
;;; The compiler notes, for the form it compiles, the global functions it
;;; names and the macros it expands (`%note-reference`, when `%references`
;;; asks it to), and a definition keeps them. A macro is expanded when its
;;; caller is compiled, so redefining it leaves every caller with the old
;;; expansion; now the world knows which those are, and marks them stale. It
;;; does not recompile them at once, as Genera's *Compile Changed
;;; Definitions* is a command: a program changing several macros should not
;;; recompile on each. `recompile-stale-definitions` does it when asked.

;;; Every definition in the world, in the order it was made.
(defun all-definitions ()
  (let ((found nil))
    (maphash (lambda (key definition) key (push definition found)) %definitions)
    (sort found #'< :key #'definition-number)))

;;; The definitions whose code calls the global function NAME, and those
;;; whose code expanded the macro NAME (SBCL's `sb-introspect` names).
(defun who-calls (name)
  (remove-if-not (lambda (definition) (member name (definition-calls definition)))
                 (all-definitions)))

(defun who-macroexpands (name)
  (remove-if-not (lambda (definition) (member name (definition-macros definition)))
                 (all-definitions)))

;;; The macro NAME was just defined: whatever expanded it, or called NAME as
;;; a function before it was a macro, was compiled for something else.
(defun %mark-stale (name)
  (maphash (lambda (key definition)
             key
             (when (or (member name (definition-macros definition))
                       (member name (definition-calls definition)))
               (setf (definition-stale definition) t)))
           %definitions))

(defun stale-definitions ()
  (remove-if-not #'definition-stale (all-definitions)))

;;; ------------------------------------------------------------------------
;;; Recompiling (sub-step 3)

;;; Evaluate DEFINITION's form again, in the package it was read in, as a
;;; new version of it with the same text and place: what a changed macro
;;; now means is compiled in. Returns the new definition.
(defun recompile-definition (definition)
  (let ((%definition-place definition)
        (%definition-source (definition-source definition))
        (*package* (or (definition-package definition) *package*)))
    (eval (definition-form definition)))
  (gethash (cons (definition-kind definition) (definition-name definition)) %definitions))

;;; Recompile every stale definition, and return what was recompiled.
;;;
;;; Macros first, in the order they were made, again and again until none
;;; is stale: recompiling a macro marks whatever expanded it stale, another
;;; macro perhaps, and a function compiled before the last macro settled
;;; would hold an old expansion. Recompiling X before the macro M it used,
;;; because X was made first, left X with M's old meaning and a stale mark
;;; (#184). Then everything else, once: only a macro's definition makes
;;; anything stale. A macro is recompiled at most four times, so two that
;;; use each other do not go round for ever.
(defun recompile-stale-definitions ()
  (let ((done nil) (turns nil))
    (flet ((again (definition)
             (let* ((key (definition-key definition))
                    (entry (assoc key turns :test #'equal)))
               (unless entry
                 (setq entry (cons key 0))
                 (push entry turns))
               (setf (cdr entry) (+ (cdr entry) 1))
               (setq done (cons (recompile-definition definition)
                                (remove key done :key #'definition-key :test #'equal))))))
      (do ((macro (%stale-macro turns) (%stale-macro turns)))
          ((null macro))
        (again macro))
      (dolist (definition (stale-definitions))
        (unless (eq (definition-kind definition) :macro)
          (again definition))))
    (nreverse done)))

;;; The first stale macro that has not been recompiled four times already.
(defun %stale-macro (turns)
  (dolist (definition (stale-definitions) nil)
    (when (and (eq (definition-kind definition) :macro)
               (< (or (cdr (assoc (definition-key definition) turns :test #'equal)) 0) 4))
      (return definition))))

(defun definition-key (definition)
  (cons (definition-kind definition) (definition-name definition)))
