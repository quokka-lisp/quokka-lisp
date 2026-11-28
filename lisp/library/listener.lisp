;;;; The listener (#141, phase 4 step G; docs/design.md, *The listener*).
;;;;
;;;; The first viewer, and the environment's first program, in the package
;;;; `QUOKKA-ENV`. It looks like the transcripts in the books, a prompt such
;;;; as `CL-USER>` followed by forms and their results, but the transcript
;;;; is live text: every form in it stays an editor, and any of them can be
;;;; changed and run again. A form that was edited does not run in place. It
;;;; runs as a new entry at the bottom, the old results are struck through,
;;;; and notes point each to the other, because in Lisp the order matters
;;;; and the transcript should stay a true record of what ran, in the order
;;;; it ran (docs/design.md decided this over a notebook's running in place).
;;;;
;;;; The listener is a *recorded* viewer (docs/display.md, decision 4): it
;;;; keeps what was written to it, and changes an entry only when something
;;;; says so. Evaluation happens in the command loop's process, inside the
;;;; command that handles the key (decision 6), with the standard streams
;;;; bound to the listener's own stream: what a form prints goes into its
;;;; entry, and what it reads, `read-line` or the debugger's prompt, is
;;;; typed into its entry while a nested command loop waits for the line.

;;; ------------------------------------------------------------------------
;;; Entries

;;; One form and what became of it: its records, the text that ran, and the
;;; package it ran in.
(defstruct (listener-entry (:constructor %make-listener-entry))
  block prompt editor output values notes ran number)

(defclass listener (viewer)
  ((transcript :accessor %listener-transcript)
   (entries :initform nil :accessor %listener-entries)
   (count :initform 0 :accessor listener-count)
   (package :initform (find-package "COMMON-LISP-USER") :accessor listener-package)
   (stream :accessor %listener-stream)
   ;; True while a form runs, so that Enter in another entry does not start
   ;; a second form inside the first.
   (busy :initform nil :accessor %listener-busy))
  (:default-initargs :title "Listener" :display-time nil))

(defmethod initialize-instance :after ((listener listener) &key)
  (let ((transcript (make-block-record)))
    (setf (%listener-transcript listener) transcript)
    (add-record (viewer-record listener) transcript)
    (setf (%listener-stream listener) (make-instance 'listener-stream :listener listener))
    (%new-entry listener)))

;;; The keys an entry's editor gives the listener rather than the browser.
;;; Shift-Enter is not among them, so it makes a new line.
(defvar %listener-keys '("Enter" "Meta+Enter" "Control+Enter"))

;;; The prompt in front of each form. A generic function, so that a person
;;; can change it from inside the world and see the next prompt change, as
;;; mock-up 03 does.
(defgeneric listener-prompt (listener stream))
(defmethod listener-prompt ((listener listener) stream)
  (format stream "~A> " (%short-package-name *package*)))

;;; A package's shortest name, as SBCL's REPL shows it: `CL-USER`.
(defun %short-package-name (package)
  (let ((best (package-name package)))
    (dolist (nickname (package-nicknames package) best)
      (when (< (length nickname) (length best)) (setq best nickname)))))

;;; The entry at the bottom, waiting for a form.
(defun %input-entry (listener) (car (%listener-entries listener)))

;;; A new entry at the bottom, its prompt drawn in the listener's package,
;;; and the caret in its editor.
(defun %new-entry (listener)
  (let* ((block (make-block-record))
         (prompt (make-block-record :style (make-text-style :fixed nil nil) :ink +accent+))
         (editor (make-instance 'editor-record :properties (list :keys %listener-keys)))
         (entry (%make-listener-entry :block block :prompt prompt :editor editor
                                      :number (setf (listener-count listener)
                                                    (+ (listener-count listener) 1)))))
    ;; The prompt and the form side by side, a row: layout in Lisp's terms
    ;; (docs/display.md), which the renderer draws as it likes.
    (let ((line (make-block-record :layout :row)))
      (add-record line prompt)
      (let ((*package* (listener-package listener)))
        (listener-prompt listener (make-record-stream prompt)))
      (add-record line editor)
      (add-record block line))
    (add-record (%listener-transcript listener) block)
    (push entry (%listener-entries listener))
    (focus-record editor)
    entry))

(defun %entry-of (listener editor)
  (find editor (%listener-entries listener) :key #'listener-entry-editor))

;;; A line of small text under an entry, such as where it ran again.
(defun %note-entry (entry text)
  (let ((note (make-block-record :note t)))
    (add-record note (let ((r (make-text-record))) (%extend-text r text 0 (length text)) r))
    (add-record (listener-entry-block entry) note)
    (push note (listener-entry-notes entry))
    note))

;;; ------------------------------------------------------------------------
;;; Keys

(defmethod viewer-input ((listener listener) event)
  (let ((target (event-target event)))
    (cond ((not (eq (event-kind event) :key)) nil)
          ;; A line a running form is reading.
          ((and target (eq target (%reading-editor listener)))
           (when (equal (event-key event) "Enter")
             (%enter-line listener target))
           t)
          ((not (typep target 'editor-record)) nil)
          (t (let ((entry (%entry-of listener target))
                   (run (or (event-modifier-p event :meta) (event-modifier-p event :control))))
               (cond ((null entry) nil)
                     ((not (equal (event-key event) "Enter")) nil)
                     ;; While a form runs, no other starts inside it.
                     ((%listener-busy listener) t)
                     ;; Enter at the bottom runs a whole form, and makes a
                     ;; new line in one that is not finished.
                     ((and (not run) (eq entry (%input-entry listener)))
                      (if (%source-complete-p (editor-text target))
                          (%run-input listener entry)
                          (%insert-newline target))
                      t)
                     ;; Cmd-Enter runs a form anywhere.
                     (run
                      (if (eq entry (%input-entry listener))
                          (%run-input listener entry)
                          (%run-again listener entry))
                      t)
                     (t (%insert-newline target) t)))))))

(defun %insert-newline (editor)
  (let* ((text (editor-text editor))
         (start (min (editor-start editor) (length text)))
         (end (max start (min (editor-end editor) (length text)))))
    (set-editor-text editor
                     (concatenate 'string (subseq text 0 start) (string #\Newline)
                                  (subseq text end))
                     :start (+ start 1))))

;;; Run the form at the bottom, and open a new entry under it.
(defun %run-input (listener entry)
  (unless (%blank-p (editor-text (listener-entry-editor entry)))
    ;; A new entry however the form is left, the debugger's "Return to
    ;; the command loop" included (review of #141).
    (unwind-protect (%run-entry listener entry)
      (%new-entry listener))))

;;; After the command loop was abandoned and started again: nothing runs,
;;; nothing reads, and the entry at the bottom waits for a form.
(defmethod viewer-restarted ((listener listener))
  (let ((stream (%listener-stream listener)))
    (setf (%listener-busy listener) nil)
    (let ((reading (%listener-reading-editor stream)))
      (when reading
        (setf (record-property reading :readonly) t)
        (setf (record-property reading :keys) nil)))
    (setf (%listener-reading-editor stream) nil
          (%listener-current-output stream) nil)
    (%clear-line-buffer stream)
    (let ((input (%input-entry listener)))
      (if (and input (listener-entry-ran input))
          (%new-entry listener)
          (when input (focus-record (listener-entry-editor input)))))))

(defun %blank-p (text)
  (every (lambda (c) (member c '(#\Space #\Tab #\Newline #\Return))) text))

;;; Run an earlier entry's form again (design, *The listener*): as a new
;;; entry above the one waiting at the bottom, with the old entry's results
;;; struck through and a note on each saying where the other is.
(defun %run-again (listener old)
  (let* ((text (editor-text (listener-entry-editor old)))
         (input (%input-entry listener))
         (entry (%new-entry-before listener input text)))
    (dolist (record (list (listener-entry-output old) (listener-entry-values old)))
      (when record (setf (record-property record :strike) t)))
    (%note-entry old (if (equal text (listener-entry-ran old))
                         "ran again below"
                         "edited · ran again below"))
    (let ((entries (%record-children (%listener-transcript listener))))
      (%note-entry entry (format nil "↑ from ~D form~:P up"
                                 (- (position (listener-entry-block entry) entries)
                                    (position (listener-entry-block old) entries)))))
    (%run-entry listener entry)
    (focus-record (listener-entry-editor input))))

;;; A new entry holding TEXT, put in front of BEFORE, which keeps its place
;;; at the bottom.
(defun %new-entry-before (listener before text)
  (let* ((entry (%new-entry listener))
         (block (listener-entry-block entry)))
    ;; `%new-entry` added it last; it goes in front of BEFORE, and BEFORE
    ;; stays the entry waiting at the bottom.
    (remove-record block)
    (insert-record (%listener-transcript listener) block
                   (position (listener-entry-block before)
                             (%record-children (%listener-transcript listener))))
    (setf (%listener-entries listener)
          (cons before (cons entry (remove before (remove entry (%listener-entries listener))))))
    (set-editor-text (listener-entry-editor entry) text)
    entry))

;;; ------------------------------------------------------------------------
;;; Evaluation

;;; The REPL's variables (CLHS 25.1.1): the last three values, forms and
;;; value lists.
(defvar * nil)
(defvar ** nil)
(defvar *** nil)
(defvar + nil)
(defvar ++ nil)
(defvar +++ nil)
(defvar / nil)
(defvar // nil)
(defvar /// nil)
(defvar - nil)

;;; Run ENTRY's text: each form in it, in the listener's package, with the
;;; standard streams bound to the listener's stream, what it prints into
;;; its output block, and its values under it.
(defun %run-entry (listener entry)
  (let* ((editor (listener-entry-editor entry))
         (text (copy-seq (editor-text editor)))
         (output (make-block-record :bar t :style (make-text-style :fixed nil nil)))
         (values-block (make-block-record :style (make-text-style :fixed nil nil))))
    (setf (listener-entry-ran entry) text)
    (add-record (listener-entry-block entry) output)
    (add-record (listener-entry-block entry) values-block)
    (setf (listener-entry-output entry) output
          (listener-entry-values entry) values-block)
    (let ((stream (%listener-stream listener)))
      (setf (%listener-current-output stream) (make-record-stream output))
      (setf (%listener-busy listener) t)
      (unwind-protect
           (let ((*package* (listener-package listener)))
             (%listener-evaluate listener text values-block)
             (setf (listener-package listener) *package*))
        (setf (%listener-busy listener) nil)
        (setf (%listener-current-output stream) nil)
        (%clear-line-buffer stream)))
    ;; An output block nothing was written to says nothing.
    (unless (record-children output) (remove-record output) (setf (listener-entry-output entry) nil))
    entry))

;;; The forms of TEXT, one after another, as SBCL's REPL takes a line.
(defun %listener-evaluate (listener text values-block)
  (let* ((%listener-evaluating t)
         (stream (%listener-stream listener))
         (*standard-output* stream)
         (*standard-input* stream)
         (*terminal-io* stream)
         (*debug-io* stream)
         (*query-io* stream)
         (*error-output* stream)
         (in (make-string-input-stream text))
         (end (list :end)))
    (do () (nil)
      (let ((form nil) (start (%stream-position in)))
        (restart-case
            (handler-bind ((serious-condition #'%listener-error))
              (setq form (read in nil end))
              (when (eq form end) (return))
              (setq - form)
              (let ((values (multiple-value-list (%eval-read-form form in start nil))))
                (setq +++ ++ ++ + + form)
                (setq /// // // / / values)
                (setq *** ** ** * * (car values))
                (%show-values values-block values)))
          (abort ()
            :report "Return to the listener."
            (fresh-line stream)
            (return)))))))

;;; An error in a form the listener runs: the debugger, if it is on, which
;;; reads its commands from the listener; otherwise the message, and back
;;; to the listener.
(defun %listener-error (condition)
  (if %debugger-enabled
      (invoke-debugger condition)
      (progn
        (with-ink (*standard-output* +error+)
          (format *standard-output* "~&; ~A~%" (%condition-text condition)))
        (invoke-restart 'abort))))

;;; The debugger cuts its backtrace at the listener's evaluation, as it does
;;; at a form typed at an outer debugger, and does not show the handler that
;;; opened it, as it does not show `error`.
(pushnew '%listener-evaluate %debugger-evaluators)
(pushnew '%listener-error %signalling-functions)

(defun %show-values (block values)
  (let ((stream (make-record-stream block)))
    (if values
        (dolist (v values)
          (prin1 v stream)
          (terpri stream))
        (with-ink (stream +dim+) (write-line "; No values" stream)))))

;;; ------------------------------------------------------------------------
;;; The listener's stream
;;;
;;; Both ways at once, as `*terminal-io*` is: what is written goes to the
;;; output block of the entry that is running, and what is read comes from
;;; lines typed into that entry. When a read finds nothing left, the stream
;;; shows the prompt a reader set (`%console-prompt`, the debugger's `0] `)
;;; and an editor for the line, and runs the command loop until the line is
;;; entered.

(defclass listener-stream (fundamental-character-input-stream
                           fundamental-character-output-stream)
  ((listener :initarg :listener :reader %stream-listener)
   (output :initform nil :accessor %listener-current-output)
   (buffer :initform "" :accessor %line-buffer)
   (at :initform 0 :accessor %line-at)
   (reading :initform nil :accessor %listener-reading-editor)
   (entered :initform nil :accessor %line-entered)
   ;; Lines written since the page was last told (`%note-lines`).
   (lines :initform 0 :accessor %lines-unsent)))

(defun %reading-editor (listener)
  (%listener-reading-editor (%listener-stream listener)))

;;; Output, to the running entry's block; with no entry running, to the
;;; bottom of the transcript.
(defun %listener-out (stream)
  (or (%listener-current-output stream)
      (setf (%listener-current-output stream)
            (make-record-stream (%listener-transcript (%stream-listener stream))))))

;;; A form that prints is seen printing. Nothing was shown until a form
;;; returned, unless it called `finish-output` itself: a loop that printed
;;; a line a second showed nothing for a minute and then sixty lines. The
;;; page is told every `%lines-a-flush` lines; every line would be a
;;; message to the host for each, which a form printing ten thousand lines
;;; would pay ten thousand times (#166).
(defvar %lines-a-flush 20)

(defun %note-lines (stream newline)
  (when newline
    (setf (%lines-unsent stream) (+ (%lines-unsent stream) 1))
    (when (>= (%lines-unsent stream) %lines-a-flush)
      (setf (%lines-unsent stream) 0)
      (flush-display))))

(defmethod stream-write-char ((stream listener-stream) char)
  (write-char char (%listener-out stream))
  (%note-lines stream (eql char #\Newline))
  char)
(defmethod stream-write-string ((stream listener-stream) string &optional (start 0) end)
  (let ((end (or end (length string))))
    (write-string string (%listener-out stream) :start start :end end)
    (%note-lines stream (position #\Newline string :start start :end end))
    string))
(defmethod stream-line-column ((stream listener-stream))
  (stream-line-column (%listener-out stream)))

;;; A drawing written to the listener goes into the running entry
;;; (`ui-drawing.lisp`).
(defmethod stream-record-stream ((stream listener-stream)) (%listener-out stream))

;;; Show what was written so far: a form that prints and then computes for a
;;; long time is seen printing.
(defmethod stream-force-output ((stream listener-stream))
  (setf (%lines-unsent stream) 0)
  (flush-display)
  nil)
(defmethod stream-finish-output ((stream listener-stream))
  (setf (%lines-unsent stream) 0)
  (flush-display)
  nil)

(defmethod stream-read-char ((stream listener-stream))
  (when (>= (%line-at stream) (length (%line-buffer stream)))
    (%await-line stream))
  (if (>= (%line-at stream) (length (%line-buffer stream)))
      :eof
      (prog1 (char (%line-buffer stream) (%line-at stream))
        (setf (%line-at stream) (+ (%line-at stream) 1)))))

(defmethod stream-unread-char ((stream listener-stream) char)
  (declare (ignore char))
  (setf (%line-at stream) (max 0 (- (%line-at stream) 1)))
  nil)

(defmethod stream-listen ((stream listener-stream))
  (< (%line-at stream) (length (%line-buffer stream))))
(defmethod stream-read-char-no-hang ((stream listener-stream))
  (if (stream-listen stream) (stream-read-char stream) nil))

(defmethod stream-clear-input ((stream listener-stream))
  (%clear-line-buffer stream))

(defun %clear-line-buffer (stream)
  (setf (%line-buffer stream) "" (%line-at stream) 0))

;;; Wait for a line: show the prompt, an editor to type it in, and run the
;;; command loop until Enter is pressed in it. At the end of the host's
;;; events, the buffer stays empty, which the reader sees as end of file.
(defun %await-line (stream)
  (let* ((out (%listener-out stream))
         (prompt %console-prompt)
         (line (make-block-record :layout :row))
         (editor (make-instance 'editor-record
                                :properties (list :keys '("Enter")))))
    (setq %console-prompt "")
    (fresh-line out)
    ;; The prompt and the line side by side, as at the bottom of the
    ;; transcript.
    (unless (equal prompt "")
      (let ((text (make-block-record :ink +accent+)))
        (%extend-text (let ((r (make-text-record))) (add-record text r) r) prompt 0 (length prompt))
        (add-record line text)))
    (add-record line editor)
    (add-record (stream-container out) line)
    (setf (%listener-reading-editor stream) editor
          (%line-entered stream) nil)
    (focus-record editor)
    (flush-display)
    (unwind-protect (read-event-loop (lambda () (%line-entered stream)))
      (setf (%listener-reading-editor stream) nil)
      ;; What follows the line is written after it.
      (setf (%listener-current-output stream) (make-record-stream (stream-container out))))))

;;; Enter in the line being read: the line, with its newline, is what the
;;; reader reads next, and its editor stays as a record of what was typed.
(defun %enter-line (listener editor)
  (let ((stream (%listener-stream listener)))
    (setf (%line-buffer stream)
          (concatenate 'string (subseq (%line-buffer stream) (%line-at stream))
                       (editor-text editor) (string #\Newline))
          (%line-at stream) 0
          (%line-entered stream) t)
    (setf (record-property editor :readonly) t)
    (setf (record-property editor :keys) nil)))

;;; ------------------------------------------------------------------------
;;; The environment's surface
;;;
;;; Phase 4's surface is one listener (docs/display.md, decision 8); the
;;; window manager of phase 6 will make it a tree. The command loop shows
;;; whatever `*environment-surface*` makes, the first time it runs; the
;;; debugger is on, since the listener is a console that can answer it.

(defvar *listener* nil)

(defun %make-environment-surface ()
  (setq %debugger-enabled t)
  (setq *listener* (make-instance 'listener)))

(setq *environment-surface* #'%make-environment-surface)
