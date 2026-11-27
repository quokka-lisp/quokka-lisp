;;;; Viewers, redisplay and the command loop (#139, phase 4 step E;
;;;; docs/display.md, decisions 4, 6 and 8).
;;;;
;;;; A **viewer** is what has a place on the screen: the listener, later an
;;;; inspector, a Rolodex, a game. The window manager knows one class,
;;;; `viewer`, and only through the small protocol of generic functions in
;;;; `docs/design.md` (*Viewers, desktops and layout*): its title, how it
;;;; draws, what it does with an event. What a viewer shows comes from its
;;;; subclass. Oberon divided the work the same way, between its `Viewers`
;;;; module, which knew rectangles and messages, and `TextFrames`, which knew
;;;; text; generic functions do here what Oberon's message records did.
;;;;
;;;; The **command loop** is the environment's top level: wait for an event,
;;;; hand it to the viewer it is for, then redraw every visible viewer that
;;;; redraws after commands, and send what changed. It runs in the main
;;;; process and waits only by asking the host, so a world with nothing to
;;;; do is idle (chapter 94).

;;; ------------------------------------------------------------------------
;;; The viewer protocol

(defclass viewer ()
  ((title :initform nil :initarg :title :accessor %viewer-title)
   (record :reader viewer-record)
   ;; When the viewer redraws: `:command-loop`, after every event, by
   ;; calling `display-viewer` again (CLIM's `:display-time`); or nil, never
   ;; by itself, for a viewer that keeps what was written to it, as the
   ;; listener does.
   (display-time :initform :command-loop :initarg :display-time
                 :accessor viewer-display-time)))

;;; A viewer's own record, the root of its tree, knows the viewer.
(defclass viewer-root (viewer-record)
  ((viewer :initarg :viewer :reader record-viewer)))

(defmethod initialize-instance :after ((viewer viewer) &key)
  (setf (slot-value viewer 'record)
        (make-instance 'viewer-root :viewer viewer
                                    :properties (list :title (viewer-title viewer)))))

(defgeneric viewer-title (viewer))
(defmethod viewer-title ((viewer viewer))
  (or (%viewer-title viewer)
      (string-capitalize (symbol-name (class-name (class-of viewer))))))

;;; Draw the viewer's contents into STREAM, as output records. Called with
;;; a fresh stream at each redisplay; must change nothing (design, *The frame
;;; hook*), since every visible viewer redraws after any event.
(defgeneric display-viewer (viewer stream))
(defmethod display-viewer ((viewer viewer) stream)
  (declare (ignore stream))
  nil)

;;; A key or an edit for the viewer, as an `event`. True if the viewer used
;;; it.
(defgeneric viewer-input (viewer event))
(defmethod viewer-input ((viewer viewer) event)
  (declare (ignore event))
  nil)

;;; Its commands, for menus and the command line: phase 5.
(defgeneric viewer-commands (viewer))
(defmethod viewer-commands ((viewer viewer)) nil)

;;; How small it can usefully be, in logical pixels, as two values.
(defgeneric viewer-minimum-size (viewer))
(defmethod viewer-minimum-size ((viewer viewer)) (values 0 0))

;;; The command loop is starting again after the computation it ran was
;;; abandoned (a Stop from the page, or a failure of the machine), which
;;; runs no `unwind-protect` cleanups: a viewer puts right whatever state a
;;; cleanup would have (review of #141). Called by `show-surface`.
(defgeneric viewer-restarted (viewer))
(defmethod viewer-restarted ((viewer viewer)) nil)

;;; What restores it, as plain data: phase 6's saving and templates.
(defgeneric viewer-state (viewer))
(defmethod viewer-state ((viewer viewer)) nil)

;;; The viewer a record is in, if it is in one.
(defun record-viewer-of (record)
  (do ((r record (record-parent r)))
      ((null r) nil)
    (when (typep r 'viewer-root) (return (record-viewer r)))))

;;; ------------------------------------------------------------------------
;;; Events
;;;
;;; An event comes from the page as an S-expression (`docs/display.md`,
;;; *Message reference*) and is read into one of these. `event-key` is the
;;; character or name a key produces, `event-position` the key's place on
;;; the keyboard as a keyword (`:key-a`, `:arrow-left`), which a game uses
;;; so its keys do not move with the keyboard's layout (design, *Keys*).

(defstruct (event (:constructor %make-event))
  kind target key code modifiers repeat text start end)

(defun event-position (event)
  (let ((code (event-code event)))
    (if (stringp code) (%code-keyword code) nil)))

(defun event-repeat-p (event) (event-repeat event))

;;; "ArrowLeft" → :ARROW-LEFT, "KeyA" → :KEY-A.
(defun %code-keyword (code)
  (let ((out (make-string-output-stream)))
    (dotimes (i (length code))
      (let ((c (char code i)))
        (when (and (> i 0) (upper-case-p c)) (write-char #\- out))
        (write-char (char-upcase c) out)))
    (%intern-keyword (get-output-stream-string out))))

;;; Is a modifier held? `:shift`, `:control`, `:alt`, `:meta`.
(defun event-modifier-p (event modifier)
  (if (member modifier (event-modifiers event)) t nil))

;;; The text of an event as an event. Read with the standard syntax and no
;;; `#.`, since the text comes from outside; anything malformed is no
;;; event at all.
(defun %parse-event (text)
  ;; Everything, the reading and the taking apart, is under the handler: a
  ;; property list that is not one (`(:key :key "a" . 5)`) is no event,
  ;; and not an error in the command loop (review of #139).
  (handler-case
      (let ((form (with-standard-io-syntax
                    (let ((*read-eval* nil)
                          (*package* (find-package "KEYWORD")))
                      (read-from-string text)))))
        (if (and (consp form) (keywordp (car form))
                 (listp (cdr form)) (evenp (list-length (cdr form))))
            (let ((plist (cdr form)))
              (flet ((of-type (key test) (let ((v (getf plist key))) (if (funcall test v) v nil))))
                (%make-event :kind (car form)
                             :target (let ((id (getf plist :target)))
                                       (if (integerp id) (find-record id) nil))
                             :key (of-type :key #'stringp)
                             :code (of-type :code #'stringp)
                             :modifiers (remove-if-not #'keywordp (of-type :modifiers #'listp))
                             :repeat (getf plist :repeat)
                             :text (of-type :text #'stringp)
                             :start (of-type :start (lambda (n) (and (integerp n) (>= n 0))))
                             :end (of-type :end (lambda (n) (and (integerp n) (>= n 0)))))))
            nil))
    (error () nil)))

;;; The next event from the host, or nil when the host will never have one.
;;; Anything it cannot read is skipped.
(defun %next-event ()
  (do ((text (%await-event) (%await-event)))
      ((null text) nil)
    (let ((event (%parse-event text)))
      (when event (return event)))))

;;; ------------------------------------------------------------------------
;;; The surface and its layout (decision 8)
;;;
;;; The surface is a tree: a viewer, or `(:split direction (weight
;;; layout)...)`, direction `:horizontal` (side by side) or `:vertical`.
;;; Phase 4 shows one viewer; phase 6's window manager splits.

(defvar *surface* nil)
(defvar %focused-viewer nil)

(defun visible-viewers (&optional (layout *surface*))
  (cond ((null layout) nil)
        ((typep layout 'viewer) (list layout))
        (t (mapcan (lambda (part) (visible-viewers (cadr part))) (cddr layout)))))

(defun focused-viewer () %focused-viewer)

;;; The layout as the renderer reads it; with each viewer's contents if
;;; WHOLE, which marks them all as on the page.
(defun %layout-node (layout whole)
  (cond ((null layout) nil)
        ((typep layout 'viewer)
         (if whole
             (record-node (viewer-record layout))
             (let ((record (viewer-record layout)))
               (list :viewer (record-id record) (list :title (viewer-title layout))))))
        (t (list* :split (cadr layout)
                  (mapcar (lambda (part) (list (car part) (%layout-node (cadr part) whole)))
                          (cddr layout))))))

;;; Show LAYOUT, from nothing: what the page is told when the environment
;;; starts and after an interrupt, since a page that was reloaded or a
;;; command loop that was abandoned may have lost anything.
(defun show-surface (layout)
  (setq *surface* layout)
  (unless (member %focused-viewer (visible-viewers layout))
    (setq %focused-viewer (car (visible-viewers layout))))
  (clrhash %shown-records)
  (setq %display-queue nil)
  (dolist (viewer (visible-viewers layout))
    (viewer-restarted viewer)
    (%mark-shown (viewer-record viewer) nil)
    (when (viewer-display-time viewer) (%redisplay-contents viewer)))
  (%queue-message (lambda () (list :reset (%layout-node *surface* t))))
  (when %focused-record
    (push (list :focus %focused-record) %display-queue))
  layout)

;;; Arrange the viewers again, keeping their contents.
(defun set-layout (layout)
  (setq *surface* layout)
  (%queue-message (lambda () (list :layout (%layout-node *surface* nil))))
  layout)

;;; ------------------------------------------------------------------------
;;; Redisplay by reconciliation (decision 4)
;;;
;;; A viewer that redraws after commands draws itself again into fresh
;;; records, and the fresh tree is compared with the one on the page. Where
;;; a fresh record is of the same kind as the old one in its place (or has
;;; the same key), the old one stays, with its identity, and takes on what
;;; changed; only what differs is sent. This is the reconciliation React
;;; made familiar in 2013 for its "virtual DOM" (React's documentation,
;;; *Reconciliation*), done here over output records, as CLIM's incremental
;;; redisplay compares records and not what is on the screen. An editor
;;; that keeps its identity keeps its node in the page, and so its caret.

(defun redisplay (viewer)
  (%redisplay-contents viewer)
  (flush-display)
  viewer)

(defun %redisplay-contents (viewer)
  (let* ((root (viewer-record viewer))
         (fresh (make-instance 'viewer-root :viewer viewer))
         (stream (make-record-stream fresh)))
    (let ((*standard-output* stream))
      (display-viewer viewer stream))
    (%reconcile-children root fresh)
    root))

;;; Make OLD look like FRESH, keeping OLD's identity: both are records of
;;; the same kind. Answers OLD.
(defun %reconcile (old fresh)
  (unless (equal (%public-properties old) (%public-properties fresh))
    (%set-properties old (%public-properties fresh)))
  (typecase old
    (text-record
     (unless (string= (record-text old) (record-text fresh))
       (setf (fill-pointer (record-text old)) 0)
       (%extend-text old (record-text fresh) 0 (length (record-text fresh)))))
    (editor-record
     (unless (string= (editor-text old) (editor-text fresh))
       (set-editor-text old (editor-text fresh))))
    (container-record (%reconcile-children old fresh)))
  old)

;;; Replace RECORD's properties with PLIST, telling the page each one that
;;; changed; one that is gone is sent as nil.
(defun %set-properties (record plist)
  (let ((old (record-properties record))
        (changed nil))
    (do ((tail plist (cddr tail)))
        ((endp tail))
      (unless (equal (getf old (car tail) '%none) (cadr tail))
        (push (car tail) changed)))
    (do ((tail old (cddr tail)))
        ((endp tail))
      (when (eq (getf plist (car tail) '%none) '%none)
        (push (car tail) changed)))
    (setf (record-properties record) (copy-list plist))
    (dolist (name (nreverse changed))
      (%note-change record (list :set name)))))

;;; Can OLD stand for FRESH? Same kind and same key.
(defun %same-record-p (old fresh)
  (and (eq (record-kind old) (record-kind fresh))
       (eq (class-of old) (class-of fresh))
       (equal (record-key old) (record-key fresh))))

(defun %reconcile-children (old fresh)
  (let* ((olds (record-children old))
         (used nil)
         (wanted
           ;; For each fresh child, the old one that will stand for it, or
           ;; the fresh child itself.
           (let ((i 0))
             (mapcar (lambda (child)
                       (let ((match (if (record-key child)
                                        (find-if (lambda (o) (and (not (member o used))
                                                                  (%same-record-p o child)))
                                                 olds)
                                        (let ((o (nth i olds)))
                                          (if (and o (not (member o used))
                                                   (%same-record-p o child))
                                              o
                                              nil)))))
                         (setq i (+ i 1))
                         (if match
                             (progn (push match used) (%reconcile match child))
                             child)))
                     (record-children fresh)))))
    ;; Old children no fresh one matched go.
    (dolist (o olds)
      (unless (member o used) (remove-record o)))
    ;; Then put each in its place: one already there stays; one out of
    ;; place, or new, is inserted there.
    (let ((i 0))
      (dolist (child wanted)
        (let ((children (%record-children old)))
          (unless (and (< i (length children)) (eq (aref children i) child))
            (when (record-parent child)
              (if (eq (record-parent child) old)
                  (remove-record child)
                  (%detach child)))
            (insert-record old child i)))
        (setq i (+ i 1))))
    old))

;;; Take a fresh record out of the fresh tree it was drawn in, which is
;;; thrown away, without telling the page about a tree it never saw.
(defun %detach (record)
  (let* ((parent (record-parent record))
         (children (%record-children parent))
         (at (position record children)))
    (when at
      (do ((i at (+ i 1)))
          ((>= i (- (length children) 1)))
        (setf (aref children i) (aref children (+ i 1))))
      (decf (fill-pointer children)))
    (setf (record-parent record) nil)))

;;; ------------------------------------------------------------------------
;;; The command loop (decision 6)

;;; Handle one event: an edit updates its editor first, as the page already
;;; shows it; then the viewer it is for is given it. An event for no record
;;; goes to the focused viewer.
(defun %dispatch-event (event)
  (let* ((target (event-target event))
         (viewer (if target (record-viewer-of target) %focused-viewer)))
    (when (and (eq (event-kind event) :edit) (typep target 'editor-record)
               (event-text event))
      (set-editor-text target (event-text event)
                       :start (or (event-start event) (length (event-text event)))
                       :end (or (event-end event) (event-start event)
                                (length (event-text event)))
                       :notify nil))
    ;; A key in an editor says where the caret is, which may have moved
    ;; since the last edit.
    (when (and (eq (event-kind event) :key) (typep target 'editor-record)
               (integerp (event-start event)))
      (setf (editor-start target) (event-start event)
            (editor-end target) (if (integerp (event-end event))
                                    (event-end event)
                                    (event-start event))))
    (when viewer
      (when (member viewer (visible-viewers)) (setq %focused-viewer viewer))
      (viewer-input viewer event))))

;;; After an event: every visible viewer that redraws after commands does,
;;; and the page is told what changed.
(defun %after-event ()
  (dolist (viewer (visible-viewers))
    (when (eq (viewer-display-time viewer) :command-loop)
      (%redisplay-contents viewer)))
  (flush-display))

;;; One event, with an `abort` restart that returns to the loop: an error a
;;; viewer does not handle is reported in the viewer and forgotten, and the
;;; loop goes on. Not an error in a form the listener is running, which is
;;; the listener's to show, or its debugger's (`%listener-evaluating`).
(defvar %listener-evaluating nil)

(defun %handle-event (event)
  (restart-case
      (handler-bind ((error (lambda (condition)
                              (unless %listener-evaluating
                                (%report-command-error condition)
                                (invoke-restart 'abort)))))
        (%dispatch-event event)
        (%after-event))
    (abort ()
      :report "Return to the command loop."
      (flush-display)
      nil)))

;;; Where a command's error is shown: at the end of the focused viewer.
(defun %report-command-error (condition)
  (let ((viewer %focused-viewer))
    (when viewer
      (let ((stream (make-record-stream (viewer-record viewer))))
        (with-ink (stream +error+)
          (format stream "~&; ~A~%" (%condition-text condition)))))))

;;; Run events until DONE returns true or the host has no more; whether DONE
;;; did. The command loop runs one with a DONE that never does; a stream
;;; waiting for a line runs one inside a form (decision 6), so that editing,
;;; and the other viewers, go on while the form waits.
(defun read-event-loop (done)
  (do ()
      ((funcall done) t)
    (let ((event (%next-event)))
      (if event
          (%handle-event event)
          (return nil)))))

;;; The environment's top level, which a page starts by name (chapter 94):
;;; show the surface, then handle events for as long as there are any.
(defvar *environment-surface* nil)

(defun %run-environment (ignored)
  (declare (ignore ignored))
  (unless *surface*
    (setq *surface* (and *environment-surface* (funcall *environment-surface*))))
  (show-surface *surface*)
  (flush-display)
  (read-event-loop (lambda () nil))
  :no-more-events)
