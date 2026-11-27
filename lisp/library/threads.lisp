;;;; threads.lisp — processes, as a program uses them (phase 3, step E,
;;;; #34; the design is `docs/processes.md`)
;;;;
;;;; The machine runs several processes, taking turns (`crate::process`):
;;;; each has its own stack and registers and its own special bindings, and
;;;; the machine switches between them at the end of a slice, when one
;;;; yields, when one waits on the host, and when one ends. This file is what
;;;; a program sees of that: threads with names and results, locks, and
;;;; waiting for something to happen.
;;;;
;;;; Genera called them processes, and so does the design; the names here
;;;; are SBCL's, from `sb-thread`, so that code written for SBCL — or for
;;;; bordeaux-threads, whose SBCL backend is these functions — reads the
;;;; same. They are in the package `QUOKKA-THREAD` (`packages.lisp`).
;;;;
;;;; Waiting is a loop that yields until what it waits for has happened: the
;;;; scheduler is round robin, so a waiting thread costs a few steps a turn.
;;;; A primitive is one step and no switch falls inside one, so
;;;; `%compare-and-swap-svref` is atomic, and the locks are built on it.

;;; A thread: its name, the machine's process, and how it ended — its
;;; values, or the condition that ended it, or `:terminated`.
(defstruct (thread (:constructor %make-thread-object (name))
                   (:predicate threadp)
                   (:copier nil))
  name
  process
  (values nil)
  (ended-by nil))

(defmethod print-object ((thread thread) stream)
  (print-unreadable-object (thread stream :type t)
    (format stream "~S ~A" (thread-name thread) (%thread-state-name thread))))

(defun %thread-state-name (thread)
  (let ((process (thread-process thread)))
    (if process
        (symbol-name (%process-state process))
        "STARTING")))


;;; SBCL's conditions for what goes wrong with a thread.
(define-condition thread-error (error)
  ((thread :initarg :thread :reader thread-error-thread)))

(define-condition join-thread-error (thread-error) ()
  (:report (lambda (condition stream)
             (format stream "The thread ~S did not finish normally."
                     (thread-name (thread-error-thread condition))))))

;;; A new thread that calls FUNCTION with ARGUMENTS (one argument may be
;;; given bare), as SBCL's `make-thread`. It starts when the scheduler next
;;; comes round to it.
(defun make-thread (function &key name arguments)
  (unless (functionp function)
    (error 'type-error :datum function :expected-type 'function))
  (setq %threads-made t)
  (let* ((thread (%make-thread-object (or name "Anonymous thread")))
         (arguments (if (listp arguments) arguments (list arguments)))
         ;; The call is in tail position, so this closure's frame is the
         ;; base thread's own, `%thread-run`, which a backtrace leaves out.
         (process (%make-process (lambda () (%thread-run thread function arguments)))))
    (setf (thread-process thread) process)
    (%set-process-data process thread)
    thread))

;;; A thread's base: its own compiler state, an `abort` restart that ends
;;; it, and a note on the console if an error nobody handled ended it.
;;;
;;; The compiler keeps its work in progress in the globals bound here (#34,
;;; decision 5). Bound dynamically at the base of every thread, the
;;; compiler's `setq` of one writes that thread's own binding, which the
;;; machine swaps out whenever another process runs, so threads compiling
;;; at the same time do not meet. The compiler itself is unchanged, and so
;;; is what it produces. A local `special` declaration and not `progv`,
;;; which binds a symbol a call, and would put a frame each into every
;;; thread's backtrace.
;;;
;;; The outermost handler is reached only by a condition nobody else
;;; handled, so recording the condition there and declining is how the
;;; thread learns why it is about to be aborted.
(defun %thread-run (thread function arguments)
  (%set-process-data (%current-process) thread)
  (let ((%code nil) (%constants nil) (%constant-count nil) (%constant-table nil)
        (%pending-name nil) (%toplevel-form nil) (%references nil)
        ;; Its own console buffer (#34): a line it reads is its own.
        (%console-input (make-string-input-stream "")))
    (declare (special %code %constants %constant-count %constant-table
                      %pending-name %toplevel-form %references))
    (let ((*standard-input* %console-input)
          (unhandled nil))
      (restart-case
          (handler-bind ((error (lambda (condition) (setq unhandled condition))))
            (setf (thread-values thread) (multiple-value-list (apply function arguments))))
        (abort ()
          :report "Terminate this thread."
          (setf (thread-ended-by thread) (or unhandled :aborted))
          (when unhandled
            (format *error-output* "~&; thread ~S ended: ~A~%"
                    (thread-name thread) (%condition-text unhandled)))
          nil)))))

;;; The thread running now. The main process, which the host's calls run
;;; in, gets a thread object the first time anyone asks.
(defun current-thread ()
  (%process-thread (%current-process)))

(defun %process-thread (process)
  (or (%process-data process)
      (let ((thread (%make-thread-object (if (%process-main-p process) "main thread" "thread"))))
        (setf (thread-process thread) process)
        (%set-process-data process thread)
        thread)))

(defun main-thread ()
  (%process-thread (find-if #'%process-main-p (%all-processes))))

;;; Every thread that has not ended.
(defun list-all-threads ()
  (mapcar #'%process-thread (%all-processes)))

(defun thread-alive-p (thread)
  (member (%process-state (thread-process thread)) '(:running :runnable :waiting)))

(defun thread-yield () (%process-yield))

;;; Yield until TEST, a function, says yes. While the main process waits
;;; like this, a background thread may read the console (`%await-console`):
;;; the main process is not reading it, and a thread it waits for may need
;;; to, as a thread stopped in the debugger does.
(defun %wait-until (test)
  (if (and %threads-made (%process-main-p (%current-process)))
      (let ((outer %main-waiting))
        (unwind-protect
             (progn (setq %main-waiting t)
                    (do () ((funcall test)) (thread-yield)))
          (setq %main-waiting outer)))
      (do () ((funcall test)) (thread-yield))))

;;; The console is the main process's. A background thread that wants a
;;; line waits until the main process is itself waiting, and the console is
;;; free, or held by a thread that has ended; then takes it, atomically. In
;;; SBCL too a background thread waits for the terminal.
(defun %await-console ()
  (let ((me (%current-process)))
    (unless (or (%process-main-p me) (eq (svref %console-owner 0) me))
      (%wait-until (lambda () (and %main-waiting (%take-console me)))))))

(defun %take-console (me)
  (let ((owner (svref %console-owner 0)))
    (and (or (null owner)
             (not (member (%process-state owner) '(:running :runnable :waiting))))
         (eq owner (%compare-and-swap-svref %console-owner 0 owner me)))))

;;; Let the console go after a line, unless the reader holds it for longer.
(defun %release-console ()
  (when (and (not %holding-console) (eq (svref %console-owner 0) (%current-process)))
    (setf (svref %console-owner 0) nil)))

;;; Wait for THREAD to end, and return its values. A thread that did not
;;; finish — an error ended it, or it was terminated — gives DEFAULT if one
;;; was given, and otherwise is a `join-thread-error`, as in SBCL.
(defun join-thread (thread &key (default nil default-p) timeout)
  (declare (ignore timeout))
  (when (eq thread (current-thread))
    (error 'join-thread-error :thread thread))
  (%wait-until (lambda () (not (thread-alive-p thread))))
  (if (and (eq (%process-state (thread-process thread)) :finished)
           (null (thread-ended-by thread)))
      (values-list (thread-values thread))
      (if default-p
          default
          (error 'join-thread-error :thread thread))))

;;; End THREAD: it unwinds to its base, running its cleanups, as Genera's
;;; `process-kill` does. The main thread cannot be ended.
(defun terminate-thread (thread)
  (when (%process-main-p (thread-process thread))
    (error 'thread-error :thread thread))
  (%process-terminate (thread-process thread))
  thread)

;;; ------------------------------------------------------------------------
;;; Locks

;;; A mutex: its name, and a cell holding its owner, which only an atomic
;;; compare-and-swap changes from nil.
(defstruct (mutex (:constructor make-mutex (&key name))
                  (:copier nil))
  name
  (cell (vector nil)))

(defun mutex-owner (mutex) (svref (mutex-cell mutex) 0))

;;; Take MUTEX, waiting while another thread has it; with WAITP false, give
;;; up at once instead and return nil. Taking a mutex one already holds is
;;; an error, as in SBCL, since it would wait for ever.
(defun grab-mutex (mutex &key (waitp t) timeout)
  (declare (ignore timeout))
  (let ((me (current-thread)))
    (when (eq (mutex-owner mutex) me)
      (error "~S is already held by this thread." mutex))
    (if waitp
        (progn (%wait-until (lambda () (null (%compare-and-swap-svref (mutex-cell mutex) 0 nil me))))
               t)
        (null (%compare-and-swap-svref (mutex-cell mutex) 0 nil me)))))

(defun release-mutex (mutex &key (if-not-owner :punt))
  (if (eq (mutex-owner mutex) (current-thread))
      (progn (setf (svref (mutex-cell mutex) 0) nil) t)
      (if (eq if-not-owner :error)
          (error "~S is not held by this thread." mutex)
          nil)))

;;; The mutex is taken inside the `unwind-protect`, and the cleanup lets it
;;; go only if this thread holds it: a thread ended between taking it and
;;; entering the body does not leave it held for ever.
(defmacro with-mutex ((mutex &key (wait-p t)) &body body)
  (let ((m (gensym "MUTEX")))
    `(let ((,m ,mutex))
       (unwind-protect
            (when (grab-mutex ,m :waitp ,wait-p) ,@body)
         (when (eq (mutex-owner ,m) (current-thread)) (release-mutex ,m))))))

;;; A wait queue: threads waiting to be told that something has changed,
;;; each with a token the notifier sets. The mutex given to `condition-wait`
;;; is held by whoever changes the queue, as SBCL's protocol has it.
(defstruct (waitqueue (:constructor make-waitqueue (&key name))
                      (:copier nil))
  name
  (waiters nil))

;;; Release MUTEX, wait until notified, and take MUTEX again. A thread
;;; ended while it waits takes its token out of the queue, or the next
;;; notification would go to it and be lost.
(defun condition-wait (queue mutex &key timeout)
  (declare (ignore timeout))
  (let ((token (vector nil)))
    (setf (waitqueue-waiters queue) (append (waitqueue-waiters queue) (list token)))
    (release-mutex mutex)
    (unwind-protect (%wait-until (lambda () (svref token 0)))
      (unless (svref token 0)
        (setf (waitqueue-waiters queue) (remove token (waitqueue-waiters queue)))))
    (grab-mutex mutex)
    t))

;;; Wake N of the threads waiting on QUEUE, the longest-waiting first.
(defun condition-notify (queue &optional (n 1))
  (dotimes (i n)
    (let ((token (pop (waitqueue-waiters queue))))
      (when token (setf (svref token 0) t))))
  nil)

(defun condition-broadcast (queue)
  (condition-notify queue (length (waitqueue-waiters queue))))
