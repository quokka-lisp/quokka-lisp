;;;; Output records (#138, phase 4 step D; docs/display.md, decisions 3 and 7).
;;;;
;;;; What a program writes to a viewer becomes **output records**: objects in
;;;; the world, in a tree for each viewer, each with a number that names it
;;;; to the page. The page never sees a record; it sees **messages** that
;;;; describe records and what changed in them, printed as S-expressions and
;;;; handed to the host with `%display` (`docs/display.md`, decision 1). This
;;;; file has the records, their styles and inks, the queue of messages and
;;;; the stream that turns what `format` writes into text records. It is the
;;;; lower half of the platform package `QUOKKA-UI`; `ui-viewers.lisp` is the
;;;; upper half.
;;;;
;;;; This is CLIM's design, the Common Lisp Interface Manager of 1991 and
;;;; McCLIM today: output recording streams that keep a history of records,
;;;; and a backend that draws records on whatever screen there is. CLIM's
;;;; backends are in Lisp and draw with X; ours is in the page and draws with
;;;; the DOM, so the records cross a boundary CLIM's never did, and each one
;;;; needs a name: its **identity**, a number never given to another record.

;;; ------------------------------------------------------------------------
;;; Records

(defvar %record-count 0)

;;; Every record has an identity, a parent once it is in a tree, properties
;;; the renderer is told (its style and ink, an editor's keys), an optional
;;; key that redisplay matches records by (`ui-viewers.lisp`), and the
;;; function that drew it (decision 7). `%shown` says what the page knows of
;;; it: nil, nothing; `:pending`, a message that will show it is queued; t,
;;; it is on the page.
(defclass output-record ()
  ((id :reader record-id)
   (parent :initform nil :accessor record-parent)
   (properties :initform nil :initarg :properties :accessor record-properties)
   (key :initform nil :initarg :key :reader record-key)
   (drawn-by :initform nil :initarg :drawn-by :accessor record-drawn-by)
   (%shown :initform nil :accessor %record-shown)
   ;; True while a change to its text is queued, so that it is queued once.
   (%noted :initform nil :accessor %record-noted)))

(defmethod initialize-instance :after ((record output-record) &key)
  (setf (slot-value record 'id) (setq %record-count (+ %record-count 1)))
  (unless (record-drawn-by record)
    (setf (record-drawn-by record) (%drawing-function))))

;;; The kind of node the renderer draws for a record (decision 3).
(defgeneric record-kind (record))

;;; A run of text in one style. Its text grows as a stream writes to it.
(defclass text-record (output-record)
  ((text :initform (make-array 16 :element-type 'character :adjustable t :fill-pointer 0)
         :reader record-text)
   ;; How many characters of the text the page has, or nil when that is
   ;; not known and the whole text must be sent: a flush sends what was
   ;; written since the last one, not everything again (#166).
   (%sent :initform nil :accessor %text-sent)))
(defmethod record-kind ((record text-record)) :text)

;;; A record with children, kept in order in an adjustable vector.
(defclass container-record (output-record)
  ((children :initform (make-array 4 :adjustable t :fill-pointer 0)
             :reader %record-children)))

(defun record-children (record)
  (coerce (%record-children record) 'list))

;;; Children stacked downwards; text inside keeps its line breaks.
(defclass block-record (container-record) ())
(defmethod record-kind ((record block-record)) :block)

;;; Text the person edits in place (decision 5). The browser edits it and
;;; reports every change as an `:edit` event; the record keeps what the
;;; browser shows, and the selection. Its key table is in its properties.
(defclass editor-record (output-record)
  ((text :initform "" :initarg :text :accessor editor-text)
   (start :initform 0 :accessor editor-start)
   (end :initform 0 :accessor editor-end)))
(defmethod record-kind ((record editor-record)) :editor)

;;; A viewer's area in the layout, holding its contents.
(defclass viewer-record (container-record) ())
(defmethod record-kind ((record viewer-record)) :viewer)

(defun make-text-record (&rest properties)
  (make-instance 'text-record :properties properties))
(defun make-block-record (&rest properties)
  (make-instance 'block-record :properties properties))

;;; A record's property, and changing one: the page is told.
(defun record-property (record name)
  (getf (record-properties record) name))

(defun %set-record-property (record name value)
  (setf (getf (record-properties record) name) value)
  (%note-change record (list :set name))
  value)

(defsetf record-property %set-record-property)

;;; ------------------------------------------------------------------------
;;; The code that drew it (decision 7)
;;;
;;; Phase 5 offers "Show the code that drew this" on any output, so each
;;; record keeps the function that made it: the innermost function on the
;;; stack that is not part of the output machinery. A function belongs to
;;; the machinery when its name is in the standard's package or one of
;;; Quokka's own system packages; the environment's `QUOKKA-ENV`, and every
;;; program's package, are not machinery, so the listener's prompt names
;;; `listener-prompt` and a line `dump-db` printed names `dump-db`. Frames
;;; are read one at a time by `%frame-callee`, which conses nothing; a
;;; record is made per run of text, not per character, and the first
;;; outside function is usually a few frames out.

(defvar %machinery-package-names
  '("COMMON-LISP" "KEYWORD" "QUOKKA" "QUOKKA-GRAY" "QUOKKA-UI" "QUOKKA-MOP"
    "QUOKKA-DEBUG" "QUOKKA-THREAD"))
(defvar %machinery-packages nil)

(defun %drawing-function ()
  (do ((i 1 (+ i 1)))
      ((> i 64) nil)
    (let ((callee (%frame-callee i)))
      (cond ((null callee) (return nil))
            ((%drawing-function-p callee) (return callee))))))

;;; Is CALLEE a function a person wrote or can browse: one named by a symbol
;;; outside the machinery? A method counts by its generic function's name;
;;; a generic function itself does not, since the method below it on the
;;; stack is the code that drew. Local and anonymous functions do not: the
;;; function they are inside is the definition to show.
(defun %drawing-function-p (callee)
  (let ((symbol (%drawing-name-symbol
                 (if (%funcallable-instance-p callee) nil (%function-name callee)))))
    (and symbol
         (let ((package (symbol-package symbol)))
           (and package
                (not (member package
                             (or %machinery-packages
                                 (setq %machinery-packages
                                       (remove nil (mapcar #'find-package
                                                           %machinery-package-names)))))))))))

(defun %drawing-name-symbol (name)
  (cond ((null name) nil)
        ((symbolp name) name)
        ((and (consp name) (eq (car name) 'setf) (consp (cdr name))) (cadr name))
        ((and (consp name) (eq (car name) :method) (consp (cdr name)))
         (%drawing-name-symbol (cadr name)))
        (t nil)))

;;; ------------------------------------------------------------------------
;;; Text styles and inks (design, *Styles, not CSS*)
;;;
;;; A text style is CLIM's: a family (`:fixed`, `:sans`, `:serif`), a face
;;; (`:roman`, `:bold`, `:italic`, `:bold-italic`) and a size (`:small`,
;;; `:normal`, `:large`, `:huge`), any of them nil for "as around it". An
;;; ink is a name the theme gives a colour to (`:ink`, `:dim`, `:accent`,
;;; `:error`, `:success`, `:warning`, and the palette's `:coral`, `:amber`,
;;; `:teal`, `:blue`, `:purple`, `:pink`), so that a dark theme can make
;;; it darker. Neither is ever CSS: the renderer turns both into classes.

(defun make-text-style (family face size) (list family face size))
(defun text-style-family (style) (first style))
(defun text-style-face (style) (second style))
(defun text-style-size (style) (third style))

(defvar *default-text-style* (make-text-style nil nil nil))

(defconstant +ink+ :ink)
(defconstant +dim+ :dim)
(defconstant +accent+ :accent)
(defconstant +error+ :error)
(defconstant +success+ :success)
(defconstant +warning+ :warning)
(defconstant +coral+ :coral)
(defconstant +amber+ :amber)
(defconstant +teal+ :teal)
(defconstant +blue+ :blue)
(defconstant +purple+ :purple)
(defconstant +pink+ :pink)

;;; ------------------------------------------------------------------------
;;; Trees, and the messages that follow them
;;;
;;; Changing a record that is on the page queues a note of the change; the
;;; notes become messages when the queue is flushed, and each message is
;;; written from the record as it is *then*. So a text record appended and
;;; then written to a hundred times before the flush is sent once, with all
;;; its text, and a record whose text changed many times is sent once, as
;;; it ends up. The queue is the world's, not a viewer's, so that one
;;; `%display` carries everything one command changed.

(defvar %display-queue nil)

(defun %note-change (record note)
  (when (eq (%record-shown record) t)
    (cond ((eq (car note) :text)
           ;; One note per record per flush, however often it changes.
           (unless (%record-noted record)
             (setf (%record-noted record) t)
             (push (list :text record) %display-queue)))
          (t (push (cons (car note) (cons record (cdr note))) %display-queue)))))

;;; Records the page knows, by number, so that an event can name one
;;; (`find-record`). A record enters when it is sent (`record-node`) and
;;; leaves when it is taken away, as in the renderer's own map.
(defvar %shown-records (make-hash-table))

(defun find-record (id) (gethash id %shown-records))

;;; Mark RECORD and everything in it as on its way to the page, or, with
;;; nil, as gone from it.
(defun %mark-shown (record state)
  (setf (%record-shown record) state)
  (unless state (remhash (record-id record) %shown-records))
  (when (typep record 'container-record)
    (let ((children (%record-children record)))
      (dotimes (i (length children))
        (%mark-shown (aref children i) state)))))

;;; A child added to a parent on its way to the page is noted as `:add`;
;;; where it goes among the children the page already has is worked out
;;; when the queue is flushed (`%message`), from the tree as it is then, so
;;; that a record added and taken away again before the flush cannot throw
;;; the places of the others out.
(defun add-record (parent child)
  (setf (record-parent child) parent)
  (vector-push-extend child (%record-children parent))
  (%note-added parent child)
  child)

(defun insert-record (parent child index)
  (let* ((children (%record-children parent))
         (count (length children)))
    (setf (record-parent child) parent)
    (vector-push-extend child children)
    (do ((i count (- i 1)))
        ((<= i index))
      (setf (aref children i) (aref children (- i 1))))
    (setf (aref children index) child)
    (%note-added parent child)
    child))

(defun %note-added (parent child)
  (when (%record-shown parent)
    (%mark-shown child :pending)
    (push (list :add parent child) %display-queue)))

(defun remove-record (record)
  (let ((parent (record-parent record)))
    (when parent
      (let* ((children (%record-children parent))
             (at (position record children)))
        (when at
          (do ((i at (+ i 1)))
              ((>= i (- (length children) 1)))
            (setf (aref children i) (aref children (+ i 1))))
          (decf (fill-pointer children))))
      (setf (record-parent record) nil)
      (case (%record-shown record)
        ((t) (push (list :remove record) %display-queue))
        ;; Not on the page yet, but perhaps queued to replace one that is:
        ;; that one is to be taken away instead (review of #138).
        (:pending
         (setq %display-queue
               (mapcar (lambda (note)
                         (if (and (eq (car note) :replace) (eq (caddr note) record))
                             (list :remove (cadr note))
                             note))
                       %display-queue))))
      (%mark-shown record nil)))
  record)

;;; Put NEW where OLD is, in OLD's parent.
(defun replace-record (old new)
  (let ((parent (record-parent old)))
    (when parent
      (let* ((children (%record-children parent))
             (at (position old children)))
        (setf (aref children at) new)
        (setf (record-parent new) parent)
        (setf (record-parent old) nil)
        (case (%record-shown old)
          ((t) (%mark-shown new :pending)
           (push (list :replace old new) %display-queue))
          ;; Not on the page yet: the note that would have shown OLD shows
          ;; NEW instead.
          (:pending (%mark-shown new :pending)
           (setq %display-queue (subst new old %display-queue))))
        (%mark-shown old nil)))
    new))

(defun clear-records (container)
  (dolist (child (record-children container))
    (remove-record child))
  container)

;;; Text added to a text record. The run is made long enough once and the
;;; characters stored: `vector-push-extend` for each one cost 3,300 steps a
;;; character (#166).
(defun %extend-text (record string start end)
  (let* ((text (record-text record))
         (fill (fill-pointer text))
         (needed (+ fill (- end start)))
         (size (array-dimension text 0)))
    (when (> needed size)
      (adjust-array text (max needed (* 2 size))))
    (%set-fill text needed)
    (do ((i start (+ i 1))
         (j fill (+ j 1)))
        ((>= i end))
      (setf (aref text j) (char string i))))
  (%note-change record '(:text)))

;;; Give a text record other text: what redisplay does to a record it
;;; keeps. The page is sent all of it, since what it has is no beginning of
;;; this.
(defun %replace-text (record string)
  (%set-fill (record-text record) 0)
  (setf (%text-sent record) nil)
  (%extend-text record string 0 (length string)))

;;; The message for a text record whose text changed: the characters the
;;; page lacks, or all of them when what it has is not known.
(defun %text-message (record)
  (let* ((text (record-text record))
         (sent (%text-sent record))
         (count (length text)))
    (setf (%text-sent record) count)
    (cond ((null sent)
           (list :set (record-id record) :text (coerce text 'simple-string)))
          ((< sent count)
           (list :append-text (record-id record)
                 (coerce (subseq text sent count) 'simple-string)))
          (t nil))))

;;; Replace an editor's text, as the world sees it: the page is told only
;;; when NOTIFY, since an edit the page reported is already on it.
(defun set-editor-text (editor text &key (start (length text)) (end start) (notify t))
  (setf (editor-text editor) text
        (editor-start editor) start
        (editor-end editor) end)
  (when notify (%note-change editor (list :set :text text :start start :end end)))
  text)

;;; The record the caret is in, remembered so that a page shown afresh
;;; (`show-surface`) is told again.
(defvar %focused-record nil)

(defun focus-record (record)
  (setq %focused-record record)
  (when (%record-shown record)
    (push (list :focus record) %display-queue))
  record)

;;; ------------------------------------------------------------------------
;;; Nodes and messages, as the renderer reads them (docs/display.md,
;;; *Message reference*)

;;; RECORD as a node, `(kind id properties child...)`, and everything in it
;;; now on the page.
(defgeneric record-node (record))

;;; A record's properties as they cross to the page: only what the
;;; protocol carries (`docs/display.md`, decision 1), integers, floats,
;;; strings, keywords, nil and t, and proper lists of them; a ratio as a
;;; float, anything else, a circular list included, as its printed text.
(defun %public-properties (record)
  (mapcar #'%wire-value (record-properties record)))

(defun %wire-value (value)
  (%wire-value-at value 0))

;;; A value as the page can read it: numbers, strings, plain keywords, t and
;;; nil as they are, a ratio as a double, a proper list element by element,
;;; and anything else as the text it prints as. Nothing here may signal or
;;; run for ever, whatever a program put in a property: this runs inside a
;;; flush, which has emptied its queue by then (#192).
(defun %wire-value-at (value depth)
  (cond ((or (integerp value) (floatp value) (stringp value) (eq value t) (null value))
         value)
        ((keywordp value) (if (%plain-keyword-p value) value (symbol-name value)))
        ((realp value) (float value))
        ((and (consp value) (< depth 8) (%proper-list-p value))
         (let ((out nil))
           (dolist (item value (nreverse out))
             (push (%wire-value-at item (+ depth 1)) out))))
        (t (let ((*print-circle* t) (*print-level* 4) (*print-length* 16))
             (princ-to-string value)))))

;;; Does LIST end in nil within a thousand conses? Not `list-length`, which
;;; signals for a dotted list.
(defun %proper-list-p (list)
  (do ((rest list (cdr rest))
       (n 0 (+ n 1)))
      ((or (atom rest) (> n 1000)) (null rest))))

;;; Is KEYWORD's name one the page's reader takes as a single token? Its
;;; reader knows no `|...|`, so a name with a parenthesis, a quote or a
;;; space in it broke a whole batch of messages and left the page waiting
;;; for ever (#191). Such a keyword is sent as a string.
(defun %plain-keyword-p (keyword)
  (let* ((name (symbol-name keyword))
         (count (length name)))
    (and (> count 0)
         (do ((i 0 (+ i 1)))
             ((or (= i count)
                  (not (let ((c (char name i)))
                         (or (alphanumericp c) (eql c #\-) (eql c #\_) (eql c #\.)))))
              (= i count))))))

;;; The record is on the page from now on.
(defun %now-shown (record)
  (setf (%record-shown record) t)
  (setf (%record-noted record) nil)
  (setf (gethash (record-id record) %shown-records) record))

(defmethod record-node ((record output-record))
  (%now-shown record)
  (list (record-kind record) (record-id record) (%public-properties record)))

(defmethod record-node ((record text-record))
  (%now-shown record)
  (setf (%text-sent record) (length (record-text record)))
  (list :text (record-id record) (%public-properties record)
        (coerce (record-text record) 'simple-string)))

(defmethod record-node ((record container-record))
  (%now-shown record)
  (list* (record-kind record) (record-id record) (%public-properties record)
         (mapcar #'record-node (record-children record))))

(defmethod record-node ((record editor-record))
  (%now-shown record)
  (list :editor (record-id record)
        (list* :text (editor-text record) :start (editor-start record)
               :end (editor-end record) (%public-properties record))))

;;; A queued note as a message, or nil if there is nothing left to say:
;;; a record taken away before the page saw it, say.
(defun %message (note)
  (let ((record (cadr note)))
    (case (car note)
      (:add (let ((child (caddr note)))
              (if (and (eq (%record-shown child) :pending) (eq (record-parent child) record))
                  (let ((before (%shown-sibling-after child)))
                    (if before
                        (list :insert (record-id record) (record-id before) (record-node child))
                        (list :append (record-id record) (record-node child))))
                  nil)))
      ;; A replacement puts a record on the page at a place no memo was
      ;; told of, so what the memos know is dropped.
      (:replace (setq %flush-memos nil)
       (if (eq (%record-shown (caddr note)) :pending)
           (list :replace (record-id record) (record-node (caddr note)))
           nil))
      (:remove (list :remove (record-id record)))
      (:text (setf (%record-noted record) nil)
       (if (eq (%record-shown record) t) (%text-message record) nil))
      (:set (if (eq (%record-shown record) t)
                (if (cddr note)
                    (if (cdddr note)
                        (list* :set (record-id record) (mapcar #'%wire-value (cddr note)))
                        (list :set (record-id record) (caddr note)
                              (%wire-value (record-property record (caddr note)))))
                    nil)
                nil))
      (:focus (if (%record-shown record) (list :focus (record-id record)) nil))
      (:thunk (setq %flush-memos nil) (funcall (caddr note)))
      (t nil))))

;;; The first sibling after CHILD that the page has: what it goes before.
;;;
;;; A program that writes a hundred lines adds a hundred children to one
;;; parent, and each is still waiting when the one before it is sent. Looked
;;; at one at a time, each would walk every sibling after it and find
;;; nothing: 400 new records took 327 million steps (#165). So a flush keeps
;;; a *memo* for each parent it sends children of, `(parent tail last)`:
;;; no child from index TAIL on is on the page (nil while nothing is known),
;;; and LAST is where the child before this one was, since the next is
;;; nearly always the one after it. A run of new children at the end of a
;;; parent is then walked once.
(defvar %flush-memos nil)

(defun %flush-memo (parent)
  (or (%assoc-eq parent %flush-memos)
      (let ((memo (list parent nil -1)))
        (push memo %flush-memos)
        memo)))

(defun %shown-sibling-after (child)
  (let* ((parent (record-parent child))
         (children (%record-children parent))
         (memo (%flush-memo parent))
         (next (+ (caddr memo) 1))
         (at (if (and (< next (length children)) (eq (aref children next) child))
                 next
                 (position child children)))
         (tail (or (cadr memo) (length children)))
         (found nil))
    (do ((i (+ at 1) (+ i 1)))
        ((or found (>= i tail)))
      (when (eq (%record-shown (aref children i)) t)
        (setq found (aref children i))))
    (setf (caddr memo) at)
    ;; Nothing after CHILD is on the page; CHILD itself is about to be.
    (unless found (setf (cadr memo) (+ at 1)))
    found))

;;; Queue a message that is not about one record, such as a layout, as a
;;; function that makes it when the queue is flushed.
(defun %queue-message (thunk)
  (push (list :thunk nil thunk) %display-queue))

;;; The messages queued since the last flush, in order.
(defun %take-messages ()
  (let ((notes (reverse %display-queue))
        (%flush-memos nil))
    (setq %display-queue nil)
    (remove nil (mapcar #'%message notes))))

;;; Send everything queued to the page, as one `%display`. Printed with the
;;; standard syntax, so a binding of `*print-base*` or `*package*` in the
;;; program cannot change what the renderer reads.
(defun flush-display ()
  (let ((messages (%take-messages)))
    (when messages
      (%display (with-standard-io-syntax
                  (let ((*print-readably* nil))
                    (prin1-to-string messages)))))
    messages))

;;; ------------------------------------------------------------------------
;;; The recording stream
;;;
;;; A Gray stream (chapter 95) that turns what is written to it into text
;;; records at the end of a block. Writing in the same style as the last run
;;; extends it, so `(format stream "~D cards" 3)` is one record and not
;;; seven. Its style and ink are what `with-text-style` and `with-ink` bind.

(defclass record-stream (fundamental-character-output-stream)
  ((container :initarg :container :accessor stream-container)
   (current :initform nil :accessor %stream-current)
   (style :initform nil :accessor stream-text-style)
   (ink :initform nil :accessor stream-ink)
   (column :initform 0 :accessor %stream-column)))

(defun make-record-stream (container)
  (make-instance 'record-stream :container container))

;;; The text record to write to now: the last one, if it is still the
;;; container's last child and in the stream's style and ink; else a new one.
(defun %stream-run (stream)
  (let* ((current (%stream-current stream))
         (container (stream-container stream))
         (children (%record-children container))
         (count (length children)))
    (if (and current
             (> count 0)
             (eq (aref children (- count 1)) current)
             (equal (record-property current :style) (stream-text-style stream))
             (eq (record-property current :ink) (stream-ink stream)))
        current
        (let ((run (make-instance 'text-record
                                  :properties (%style-properties stream))))
          (add-record container run)
          (setf (%stream-current stream) run)))))

(defun %style-properties (stream)
  (let ((style (stream-text-style stream))
        (ink (stream-ink stream)))
    (append (if style (list :style style) nil)
            (if ink (list :ink ink) nil))))

(defmethod stream-write-char ((stream record-stream) char)
  (let ((string (make-string 1 :initial-element char)))
    (stream-write-string stream string 0 1))
  char)

(defmethod stream-write-string ((stream record-stream) string &optional (start 0) end)
  (let ((end (or end (length string))))
    (when (< start end)
      (%extend-text (%stream-run stream) string start end)
      (let ((newline (position #\Newline string :start start :end end :from-end t)))
        (setf (%stream-column stream)
              (if newline (- end newline 1) (+ (%stream-column stream) (- end start)))))))
  string)

(defmethod stream-line-column ((stream record-stream)) (%stream-column stream))

;;; Text written in BODY to STREAM (t or nil: `*standard-output*`) is in
;;; STYLE, a list `(family face size)`, CLIM's `with-text-style`; and with
;;; `with-ink`, in a theme's ink. Outside a record stream they do nothing.
(defmacro with-text-style ((stream style) &body body)
  `(%call-with-stream-option ,stream 'style ,style (lambda () ,@body)))

(defmacro with-ink ((stream ink) &body body)
  `(%call-with-stream-option ,stream 'ink ,ink (lambda () ,@body)))

(defun %call-with-stream-option (stream slot value thunk)
  (let ((stream (if (member stream '(t nil)) *standard-output* stream)))
    (if (typep stream 'record-stream)
        (let ((old (slot-value stream slot)))
          (setf (slot-value stream slot) value)
          (unwind-protect (funcall thunk)
            (setf (slot-value stream slot) old)))
        (funcall thunk))))
