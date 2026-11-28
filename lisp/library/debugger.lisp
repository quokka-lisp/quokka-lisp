;;;; debugger.lisp — the terminal debugger (phase 3, step F, #131; the
;;;; design is `docs/debugger.md`)
;;;;
;;;; An error nobody handles reaches `invoke-debugger`. With the debugger
;;;; enabled, it stops there and talks to the person at the console: it says
;;;; what went wrong, lists the restarts, and reads commands until one of them
;;;; leaves, by a restart, by retrying a frame or by returning from one. The
;;;; failed computation stays on the stack underneath the whole time, which is
;;;; what makes it possible to redefine a function and run its frame again.
;;;;
;;;; It looks and answers like SBCL's debugger, so that what a Lisp
;;;; programmer already knows works here: the same banner, the same `0] `
;;;; prompt, the same command names. Commands are symbols, read from the
;;;; console stream; a number invokes a restart; anything else is evaluated
;;;; where the error happened, and an error in that opens a debugger one
;;;; level deeper.

;;; SBCL's names for the switch, `%debugger-enabled` (`conditions.lisp`),
;;; from `sb-ext`.
(defun enable-debugger () (setq %debugger-enabled t) (values))
(defun disable-debugger () (setq %debugger-enabled nil) (values))

;;; The same switch, for a host, which calls a function of one argument:
;;; the terminal host turns the debugger on when its input is a terminal,
;;; and off when it is not, every time it starts a world, since a world
;;; saved from a terminal may next run a script.
(defun %host-debugger (on) (setq %debugger-enabled (if on t nil)))

;;; How many debuggers are open, one inside another; the prompt shows it,
;;; from 0, as SBCL's does.
(defvar %debug-level 0)

;;; What the innermost debugger is looking at: the condition, the restarts
;;; that were available when it was entered, the frames it shows (a list,
;;; innermost first) and which of them is current.
(defvar %debug-condition nil)
(defvar %debug-restarts nil)
(defvar %debug-frames nil)
(defvar %debug-frame 0)

;;; The functions that signal and enter the debugger, whose frames are not
;;; the program's: the debugger shows the stack from the frame that called
;;; the innermost of them.
(defvar %signalling-functions
  '(%debugger %debugger-session invoke-debugger error cerror signal %signal break
    %signal-machine-error %signalled))

;;; The host loop's own frames, at the bottom of every stack a person's
;;; form makes, and those `eval` makes for a form typed at the debugger.
(defvar %host-loop-functions
  '(%host-step %host-step-reading %eval-read-form eval compiled-form %thread-run))

;;; Where a form typed at a debugger is evaluated: a debugger opened by an
;;; error in it shows the frames above this, the typed form's own, and not
;;; the stack of the debugger before it, which that one still shows.
(defvar %debugger-evaluators '(%debugger-evaluate %run-debugger-command))

;;; How many frames a debugger gathers, counted from the innermost. Making
;;; a frame object for every frame of a runaway recursion cost more than the
;;; step budget from about twelve thousand frames, and the error was never
;;; shown at all (#177); it is the innermost frames a person reads.
(defvar %debugger-frame-limit 500)

;;; The frames a debugger shows: those below the innermost signalling
;;; function, above the host loop, and above a debugger's evaluation.
(defun %debugger-frames ()
  (let ((all (frames %debugger-frame-limit)))
    (let ((signalled (member-if (lambda (frame)
                                  (member (frame-name frame) %signalling-functions))
                                all)))
      (when signalled
        (setq all (member-if-not (lambda (frame)
                                   (member (frame-name frame) %signalling-functions))
                                 signalled))))
    (let ((typed (position-if (lambda (frame) (member (frame-name frame) %debugger-evaluators))
                              all)))
      (when typed (setq all (subseq all 0 typed))))
    (let ((shown (reverse all)))
      (do () ((not (and shown (member (frame-name (car shown)) %host-loop-functions))))
        (pop shown))
      (nreverse shown))))

(defun %debug-out () *debug-io*)

;;; Write a frame as SBCL does, `3: (FOO 1 2)`, its arguments printed
;;; shallowly.
;;;
;;; A frame holds whatever the program passed, and printing that can fail:
;;; an argument whose `print-object` method signals. An error here would
;;; open a debugger from inside the debugger, for each frame; the frame is
;;; named instead, as a condition whose report fails is (#181).
(defun %print-frame (index frame)
  (let ((*print-level* 4) (*print-length* 10))
    (let ((text (handler-case (prin1-to-string (frame-call frame))
                  (error () nil))))
      (format (%debug-out) "~&~D: ~A~%" index
              (or text
                  (handler-case (format nil "(~S #<error printing its arguments>)"
                                        (frame-name frame))
                    (error () "#<error printing this frame>")))))))

(defun %print-restarts (restarts)
  (format (%debug-out) "~&restarts (invokable by number or by name):~%")
  (let ((width (reduce #'max
                       (mapcar (lambda (r) (length (princ-to-string (or (restart-name r) ""))))
                               restarts)
                       :initial-value 0))
        (n 0))
    (dolist (restart restarts)
      (let ((name (if (restart-name restart) (princ-to-string (restart-name restart)) nil)))
        (format (%debug-out) "  ~D: ~A ~A~%" n
                (if name
                    (concatenate 'string "[" name
                                 (make-string (- width (length name)) :initial-element #\Space)
                                 "]")
                    (make-string (+ width 2) :initial-element #\Space))
                restart))
      (setq n (+ n 1)))))

;;; What a person sees on entering: the condition, the restarts, and the
;;; frame the debugger starts at.
(defun %debugger-banner ()
  (format (%debug-out) "~&~%debugger invoked on ~A ~S~A: ~A~%~%"
          (if (find (char (symbol-name (type-of %debug-condition)) 0) "AEIOU") "an" "a")
          (type-of %debug-condition)
          ;; Which thread stopped, when it is not the main one (#34).
          (let ((process (%current-process)))
            (if (%process-main-p process)
                ""
                (format nil " in thread ~S" (thread-name (%process-thread process)))))
          (%condition-text %debug-condition))
  (format (%debug-out) "Type HELP for debugger help.~%~%")
  (%print-restarts %debug-restarts)
  (when %debug-frames
    (format (%debug-out) "~%")
    (%print-frame %debug-frame (nth %debug-frame %debug-frames))))

;;; Enter the debugger for CONDITION: the loop that reads commands. It
;;; returns only by a non-local exit: a restart, a retried frame, a frame
;;; returned from. At the end of the console's input it does what an
;;; unhandled error did without a debugger.
(defun %debugger (condition)
  (%repair-syntax)
  (let* ((level %debug-level)
         ;; The debugger's own printing starts afresh: it is not part of
         ;; whatever printout the error interrupted.
         (%print-nesting nil)
         (%debug-level (+ level 1))
         (%debug-condition condition)
         (%debug-restarts (compute-restarts condition))
         (%debug-frames (%debugger-frames))
         (%debug-frame 0)
         (*print-readably* nil))
    ;; A debugger reads from `*debug-io*`; one a program has bound to a
    ;; stream that cannot be read would fail on every read, each failure
    ;; opening another debugger.
    (unless (input-stream-p *debug-io*)
      (return-from %debugger (%without-a-debugger condition)))
    ;; In a background thread, the console is held for the whole session,
    ;; and waited for first (#34): the banner is shown only when the person
    ;; can answer it.
    (when %threads-made
      (%await-console))
    (let ((%holding-console t))
      (unwind-protect (%debugger-session condition level)
        (let ((%holding-console nil))
          (when %threads-made (%release-console)))))))

;;; The session itself: the banner, then commands until one leaves.
(defun %debugger-session (condition level)
  ;; What was typed before the error is not an answer to it.
  (clear-input *debug-io*)
  (%debugger-banner)
  ;; The end of the input is acted on outside the level's `abort`
  ;; restart: inside it, that restart would be the innermost, and
  ;; aborting would come straight back here to read the end again.
  (do () (nil)
    (unless (restart-case (%debugger-command level)
              (abort ()
                :report (lambda (stream) (format stream "Return to debug level ~D." level))
                t))
      (return-from %debugger-session (%without-a-debugger condition)))))

;;; What a read at the end of the console's input returns.
(defvar %debugger-end (list :end))

;;; Read one command at LEVEL and do it; nil at the end of the input. A
;;; command's arguments are the rest of its line, as in SBCL: `frame 3`,
;;; `return (+ 1 2)`. Reading the command preserves the whitespace after it,
;;; or a newline ending it would be gone and the rest of the line with it.
(defun %debugger-command (level)
  (let ((in *debug-io*))
    (setq %console-prompt (format nil "~D] " level))
    (let ((form (read-preserving-whitespace in nil %debugger-end)))
      (setq %console-prompt "")
      ;; The rest of a line that held a form or a number is nothing but its
      ;; newline, which would otherwise wait to be read by whatever reads
      ;; the console next, after the debugger has gone (a command reads the
      ;; rest of its line itself).
      (unless (and (symbolp form) form (%debugger-command-p form))
        (%skip-blank-line in))
      (cond ((eq form %debugger-end) nil)
            ((and (symbolp form) form (%debugger-command-p form))
             (%run-debugger-command form (%rest-of-line in))
             t)
            ((integerp form)
             (%invoke-numbered-restart form)
             t)
            ((and (symbolp form) form (%named-restart form))
             (invoke-restart-interactively (%named-restart form))
             t)
            (t (%debugger-evaluate form) t)))))

;;; Consume the rest of the console's current line if there is nothing on
;;; it but blanks, without asking for another line.
(defun %skip-blank-line (in)
  (let ((stream (%input-stream in)))
    (when (eq stream %console-input)
      (let ((string (%string-input-stream-string stream))
            (end (%string-input-stream-end stream)))
        (do ((at (%string-input-stream-position stream) (+ at 1)))
            ((or (>= at end) (not (member (char string at) '(#\Space #\Tab #\Return #\Newline))))
             (when (>= at end)
               (setf (%string-input-stream-position stream) end))))))))

;;; The forms on the rest of the current line.
(defun %rest-of-line (in)
  (let ((line (read-line in nil "")))
    (with-input-from-string (s line)
      (let ((forms nil))
        (do ((form (read s nil %debugger-end) (read s nil %debugger-end)))
            ((eq form %debugger-end) (nreverse forms))
          (push form forms))))))

(defun %debugger-evaluate (form)
  (let ((values (multiple-value-list (eval form))))
    (dolist (v values)
      (format (%debug-out) "~&~S~%" v))
    (unless values (format (%debug-out) "~&; No values~%"))))

(defun %invoke-numbered-restart (n)
  (let ((restart (and (>= n 0) (nth n %debug-restarts))))
    (if restart
        (invoke-restart-interactively restart)
        (format (%debug-out) "~&There is no restart ~D.~%" n))))

;;; The innermost restart available on entry whose name is NAME's, whatever
;;; the package: `continue`, `store-value`, `use-value`.
(defun %named-restart (name)
  (find-if (lambda (r) (and (restart-name r)
                            (string= (symbol-name (restart-name r)) (symbol-name name))))
           %debug-restarts))

;;; ------------------------------------------------------------------------
;;; The commands

;;; Each command: its names, what it does, a line of help, and how many
;;; arguments it takes, at least and at most; in the order they were
;;; defined, which `help` lists them in.
(defvar %debugger-commands nil)

;;; Defining a command again replaces it in its place: a warm rebuild loads
;;; this file into a world that has them all, and a list that grew each time
;;; would make every rebuilt world differ from the last.
(defmacro %define-debugger-command (names lambda-list help &body body)
  (let ((required (or (position '&optional lambda-list) (length lambda-list))))
    `(%add-debugger-command
      (list ',names (lambda ,lambda-list ,@body) ,help ,required ,(length (remove '&optional lambda-list))))))

(defun %add-debugger-command (command)
  (let ((old (find (car (first command)) %debugger-commands
                   :key (lambda (c) (car (first c))) :test #'string=)))
    (if old
        (setq %debugger-commands (substitute command old %debugger-commands))
        (setq %debugger-commands (append %debugger-commands (list command))))))

(defun %find-debugger-command (symbol)
  (find-if (lambda (command)
             (member (symbol-name symbol) (first command) :test #'string=))
           %debugger-commands))

(defun %debugger-command-p (symbol)
  (and (%find-debugger-command symbol) t))

;;; Run a command, its arguments counted first: the count is checked here
;;; and not by catching the `program-error` a wrong count signals, which
;;; would also catch one from a form the command evaluates.
(defun %run-debugger-command (symbol arguments)
  (let* ((command (%find-debugger-command symbol))
         (n (length arguments)))
    (if (and (>= n (fourth command)) (<= n (fifth command)))
        (apply (second command) arguments)
        (format (%debug-out) "~&~A takes ~D to ~D arguments.~%"
                (string-downcase (car (first command))) (fourth command) (fifth command)))))

(defun %current-frame ()
  (nth %debug-frame %debug-frames))

;;; Make frame N current and show it.
(defun %select-frame (n)
  (cond ((null %debug-frames) (format (%debug-out) "~&There are no frames.~%"))
        ((and (integerp n) (>= n 0) (< n (length %debug-frames)))
         (setq %debug-frame n)
         (%print-frame n (%current-frame)))
        (t (format (%debug-out) "~&There is no frame ~S.~%" n))))

(%define-debugger-command ("HELP" "?") ()
    "Show this list."
  (format (%debug-out) "~&A number invokes that restart, and a restart's name invokes it.~%")
  (format (%debug-out) "Anything that is not a command is evaluated, and its values printed.~%")
  (format (%debug-out) "The commands:~%")
  (dolist (command %debugger-commands)
    (let ((names (format nil "~{~A~^, ~}" (mapcar #'string-downcase (first command)))))
      (format (%debug-out) "  ~A~A~A~%" names
              (make-string (max 1 (- 26 (length names))) :initial-element #\Space)
              (third command)))))

(%define-debugger-command ("BACKTRACE" "BT") (&optional count)
    "backtrace [n]: show the frames, or the first N."
  (let ((n 0))
    (dolist (frame %debug-frames)
      (when (and count (>= n count)) (return))
      (%print-frame n frame)
      (setq n (+ n 1)))))

(%define-debugger-command ("FRAME") (&optional (n %debug-frame))
    "frame n: make frame N current, and show it."
  (%select-frame n))

(%define-debugger-command ("UP" "U") ()
    "Move to the caller of the current frame."
  (if (< (+ %debug-frame 1) (length %debug-frames))
      (%select-frame (+ %debug-frame 1))
      (format (%debug-out) "~&This is the bottom frame.~%")))

(%define-debugger-command ("DOWN" "D") ()
    "Move to the frame the current one called."
  (if (> %debug-frame 0)
      (%select-frame (- %debug-frame 1))
      (format (%debug-out) "~&This is the top frame.~%")))

(%define-debugger-command ("TOP") ()
    "Move to the frame that signalled."
  (%select-frame 0))

(%define-debugger-command ("BOTTOM") ()
    "Move to the outermost frame."
  (%select-frame (max 0 (- (length %debug-frames) 1))))

(%define-debugger-command ("PRINT" "P") ()
    "Show the current frame."
  (%select-frame %debug-frame))

(%define-debugger-command ("LIST-LOCALS" "L") ()
    "Show the current frame's variables."
  (let ((frame (%current-frame)))
    (cond ((null frame) (format (%debug-out) "~&There are no frames.~%"))
          ((null (frame-locals frame)) (format (%debug-out) "~&No variables.~%"))
          (t (let ((*print-level* 4) (*print-length* 10))
               (dolist (pair (frame-locals frame))
                 (format (%debug-out) "~&  ~S = ~S~%" (car pair) (cdr pair))))))))

(%define-debugger-command ("SOURCE") ()
    "Show the definition of the current frame's function, as it was written."
  (let* ((frame (%current-frame))
         (definition (and frame (find-definition (frame-name frame) :function))))
    (cond ((null frame) (format (%debug-out) "~&There are no frames.~%"))
          ((and definition (definition-text definition))
           (format (%debug-out) "~&~A~%" (definition-text definition)))
          (t (format (%debug-out) "~&No source is kept for ~S.~%" (frame-name frame))))))

(%define-debugger-command ("ERROR") ()
    "Show the condition again."
  (format (%debug-out) "~&~A~%" (%condition-text %debug-condition)))

(%define-debugger-command ("RESTART-FRAME") ()
    "Run the current frame's call again, with its function's current definition."
  (if (%current-frame)
      (restart-frame (%current-frame))
      (format (%debug-out) "~&There are no frames.~%")))

(%define-debugger-command ("RETURN") (&optional (form nil supplied))
    "return form: return FORM's values from the current frame."
  (cond ((null (%current-frame)) (format (%debug-out) "~&There are no frames.~%"))
        ((not supplied) (format (%debug-out) "~&RETURN needs a form: return 42.~%"))
        (t (apply #'return-from-frame (%current-frame) (multiple-value-list (eval form))))))

(%define-debugger-command ("ABORT" "A") ()
    "Leave this debugger: return to the level before, or to the top level."
  (let ((abort (find 'abort %debug-restarts :key #'restart-name)))
    (if abort (invoke-restart abort) (format (%debug-out) "~&There is no abort restart.~%"))))

(%define-debugger-command ("TOPLEVEL" "TOP-LEVEL" "Q") ()
    "Return to the top level, leaving every debugger."
  (let ((abort (find 'abort (reverse %debug-restarts) :key #'restart-name)))
    (if abort (invoke-restart abort) (format (%debug-out) "~&There is no abort restart.~%"))))

(%define-debugger-command ("CONTINUE" "C") ()
    "Invoke the innermost continue restart."
  (let ((continue (find 'continue %debug-restarts :key #'restart-name)))
    (if continue
        (invoke-restart-interactively continue)
        (format (%debug-out) "~&There is no continue restart.~%"))))
