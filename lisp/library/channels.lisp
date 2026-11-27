;;;; channels.lisp — Go's channels between green threads (#34, layer 1 of
;;;; the plan on #34: channels and tasks in one world now, a pool of worker
;;;; worlds across cores later, under the same names)
;;;;
;;;; A channel carries values from the threads that send to the threads that
;;;; receive, in the order sent. As in Go, an unbuffered channel is a
;;;; meeting place: `send` waits until a receiver has taken the value. A
;;;; buffered one holds up to its capacity, and `send` waits only when it is
;;;; full. `select` waits on several at once. `spawn-task` runs a function
;;;; in a thread of its own and answers at once with a channel that will
;;;; receive the function's value: Go's result channel, which other
;;;; languages call a future.
;;;;
;;;; Common Lisp has no channels. The nearest library, ChanL, has the same
;;;; model, and its names differ only in `recv`. They are in the package
;;;; `QUOKKA-THREAD`, with the threads they connect.
;;;;
;;;; The algorithm is the one Go's runtime uses (`runtime/chan.go`). Every
;;;; operation is one critical section: it looks at the channel and either
;;;; completes at once, handing its value straight to a waiting partner or
;;;; through the buffer, or puts a *waiter* in the channel's queue of
;;;; waiting senders or receivers and then waits for a partner to complete
;;;; it. A waiter is completed only by the partner, under the lock, so a
;;;; thread waiting reads one flag that only ever goes from nil to t. The
;;;; review of this file found the first version, which matched partners
;;;; less directly, losing values and deadlocking (`docs/processes.md`).
;;;;
;;;; There is one lock for all channels. A `select` takes it once for all
;;;; its channels, so two selects can never hold one channel each and wait
;;;; for the other's, and on one processor there is nobody to contend with
;;;; it for longer than a few steps. Every operation, and the cleanup that
;;;; withdraws a waiter, is a region in which the thread cannot be ended
;;;; (`%without-interrupts`); only its waiting can be, and the cleanup then
;;;; runs whole. The second review found a thread ended between queueing a
;;;; waiter and setting up that cleanup, and one ended while letting go of
;;;; the lock, which then stayed held for ever.

(defvar %channels-mutex (make-mutex :name "channels"))

;;; BODY with the lock held, in a region where the thread cannot be ended.
(defmacro %with-channels (&body body)
  (let ((held (gensym "HELD")))
    `(%without-interrupts
       (let ((,held nil))
         (unwind-protect
              (progn (grab-mutex %channels-mutex) (setq ,held t) ,@body)
           (when ,held (release-mutex %channels-mutex)))))))

;;; Checked before the lock is taken: an error signalled with it held would
;;; reach handlers that may use a channel themselves.
(defun %check-channel (channel)
  (unless (channel-p channel)
    (error 'type-error :datum channel :expected-type 'channel)))

;;; A channel: its capacity (0, a meeting place), its buffer as a queue,
;;; whether it is closed, and its queues of waiting senders and receivers.
;;; A queue is a cons of its list and the list's last cons.
(defstruct (channel (:constructor %make-channel (capacity))
                    (:copier nil))
  capacity
  (buffer (cons nil nil))
  (count 0)
  (closed nil)
  (senders (cons nil nil))
  (receivers (cons nil nil)))

(defmethod print-object ((channel channel) stream)
  (print-unreadable-object (channel stream :type t)
    (format stream "~D/~D~:[~; closed~]"
            (channel-count channel) (channel-capacity channel) (channel-closed channel))))

(defun make-channel (&key (capacity 0))
  (unless (and (integerp capacity) (>= capacity 0))
    (error 'type-error :datum capacity :expected-type '(integer 0)))
  (%make-channel capacity))

(defun channel-closed-p (channel) (channel-closed channel))

(define-condition closed-channel-error (error)
  ((channel :initarg :channel :reader closed-channel-error-channel))
  (:report (lambda (condition stream)
             (format stream "~S is closed."
                     (closed-channel-error-channel condition)))))

;;; ------------------------------------------------------------------------
;;; Queues and waiters

(defun %queue-push (queue item)
  (let ((link (list item)))
    (if (cdr queue)
        (setf (cdr (cdr queue)) link)
        (setf (car queue) link))
    (setf (cdr queue) link)))

(defun %queue-pop (queue)
  (let ((item (pop (car queue))))
    (unless (car queue) (setf (cdr queue) nil))
    item))

;;; A thread waiting on a channel: the value it sends, or the one it is
;;; given; whether a partner has completed it (DONE), and whether that was
;;; a value (OK, false when the channel was closed under it); and, for a
;;; `select`, the selection it belongs to and which of its clauses this is.
;;; A selection is a one-slot vector holding the waiter that fired, so that
;;; the first partner to complete one of its waiters completes the select,
;;; and its other waiters are passed over.
(defstruct (%waiter (:constructor %make-waiter (value selection clause))
                    (:copier nil))
  value
  (done nil)
  (ok nil)
  selection
  clause)

;;; Can W still be completed? Not once a partner has done so, nor once
;;; another waiter of its selection has fired, nor once its thread stopped
;;; waiting (a select that went another way, a thread terminated).
(defun %live-p (w)
  (and (not (%waiter-done w))
       (or (null (%waiter-selection w))
           (null (svref (%waiter-selection w) 0)))))

;;; The first waiter in QUEUE that can still be completed, taken off it;
;;; dead ones are dropped on the way. The lock is held.
(defun %next-waiter (queue)
  (do () ((null (car queue)) nil)
    (let ((w (%queue-pop queue)))
      (when (%live-p w) (return w)))))

;;; Complete W with VALUE; OK false means the channel was closed. The lock
;;; is held.
(defun %complete (w value ok)
  (when (%waiter-selection w)
    (setf (svref (%waiter-selection w) 0) w))
  (setf (%waiter-value w) value)
  (setf (%waiter-ok w) ok)
  (setf (%waiter-done w) t))

;;; Stop waiting with W, whose thread is leaving: if no partner completed
;;; it, it never will. The lock is held. A value a partner handed to a
;;; receiver ended just before this is lost with the receiver, as it would
;;; be with any thread ended just after receiving.
(defun %cancel (w)
  (unless (%waiter-done w)
    (setf (%waiter-done w) :cancelled)))

;;; One operation: TRY, under the lock, completes it at once and answers
;;; what happened, or answers nil; then ENQUEUE, still under the lock,
;;; queues a waiter and answers it, and the thread waits for a partner to
;;; complete it. Answers TRY's result, or the waiter, which is done.
(defun %exchange (try enqueue)
  (let ((result nil) (w nil))
    (%without-interrupts
      (unwind-protect
           (progn
             (%with-channels
               (setq result (funcall try))
               (unless result (setq w (funcall enqueue))))
             (when w (%wait-until (lambda () (%waiter-done w)))))
        (when (and w (not (eq (%waiter-done w) t)))
          (%with-channels (%cancel w)))))
    (or result w)))

;;; ------------------------------------------------------------------------
;;; The operations, each one critical section that completes or waits

;;; Try to send VALUE without waiting: :sent, :closed, or nil when the
;;; channel has neither a waiting receiver nor room. The lock is held.
(defun %try-send (channel value)
  (cond ((channel-closed channel) :closed)
        ((let ((receiver (%next-waiter (channel-receivers channel))))
           (when receiver (%complete receiver value t) t))
         :sent)
        ((< (channel-count channel) (channel-capacity channel))
         (%queue-push (channel-buffer channel) value)
         (setf (channel-count channel) (+ (channel-count channel) 1))
         :sent)
        (t nil)))

;;; Try to receive without waiting: (value . t), (nil . nil) once closed
;;; and empty, or nil when nothing is there. A full buffer with a sender
;;; waiting gives its oldest value and takes the sender's in at the end, as
;;; Go's does, so the order sent is the order received. The lock is held.
(defun %try-receive (channel)
  (cond ((> (channel-count channel) 0)
         (let ((value (%queue-pop (channel-buffer channel)))
               (sender (%next-waiter (channel-senders channel))))
           (if sender
               (progn (%queue-push (channel-buffer channel) (%waiter-value sender))
                      (%complete sender nil t))
               (setf (channel-count channel) (- (channel-count channel) 1)))
           (cons value t)))
        ((let ((sender (%next-waiter (channel-senders channel))))
           (when sender
             (let ((value (%waiter-value sender)))
               (%complete sender nil t)
               (cons value t)))))
        ((channel-closed channel) (cons nil nil))
        (t nil)))

;;; What a received value means: its value and t, or nil and nil once the
;;; channel is closed. A task's failure is signalled here, where its value
;;; was wanted (`spawn-task`).
(defun %delivered (value ok)
  (if (and ok (%task-failure-p value))
      (error (%task-failure-condition value))
      (values value ok)))

;;; Send VALUE on CHANNEL: at once to a waiting receiver, into the buffer
;;; if there is room, or else wait for a receiver. Sending to a closed
;;; channel is an error, as in Go, including for a sender still waiting
;;; when the channel is closed.
(defun send (channel value)
  (%check-channel channel)
  (let ((done (%exchange (lambda () (%try-send channel value))
                         (lambda ()
                           (let ((w (%make-waiter value nil nil)))
                             (%queue-push (channel-senders channel) w)
                             w)))))
    (when (or (eq done :closed) (and (%waiter-p done) (not (%waiter-ok done))))
      (error 'closed-channel-error :channel channel))
    value))

;;; Send VALUE if it can go at once: t if it went, nil if it would have to
;;; wait. On an unbuffered channel it goes only to a receiver already
;;; waiting. Never waits.
(defun try-send (channel value)
  (%check-channel channel)
  (let ((result (%with-channels (%try-send channel value))))
    (when (eq result :closed)
      (error 'closed-channel-error :channel channel))
    (eq result :sent)))

;;; Receive the next value from CHANNEL, waiting until there is one: the
;;; value and t, or nil and nil once the channel is closed and empty.
(defun receive (channel)
  (%check-channel channel)
  (let ((got (%exchange (lambda () (%try-receive channel))
                        (lambda ()
                          (let ((w (%make-waiter nil nil nil)))
                            (%queue-push (channel-receivers channel) w)
                            w)))))
    (if (%waiter-p got)
        (%delivered (%waiter-value got) (%waiter-ok got))
        (%delivered (car got) (cdr got)))))

;;; Receive if a value is there: the value and t, or nil and nil. Never
;;; waits.
(defun try-receive (channel)
  (%check-channel channel)
  (let ((got (%with-channels (%try-receive channel))))
    (if got (%delivered (car got) (cdr got)) (values nil nil))))

;;; Close CHANNEL: no more sends. Every waiting receiver gets nil and nil,
;;; every waiting sender an error; receivers after it get what the buffer
;;; holds, then nil and nil. Closing a closed channel is an error, as in Go.
(defun close-channel (channel)
  (%check-channel channel)
  (let ((again nil))
    (%with-channels
      (if (channel-closed channel)
          (setq again t)
          (progn
            (setf (channel-closed channel) t)
            (do ((w (%next-waiter (channel-receivers channel))
                    (%next-waiter (channel-receivers channel))))
                ((null w))
              (%complete w nil nil))
            (do ((w (%next-waiter (channel-senders channel))
                    (%next-waiter (channel-senders channel))))
                ((null w))
              (%complete w nil nil)))))
    (when again
      (error 'closed-channel-error :channel channel)))
  channel)

;;; ------------------------------------------------------------------------
;;; select

;;; `(select ((receive ch v [ok]) body...) ((send ch value) body...)
;;;          (otherwise body...))`: wait until one of the clauses can go, do
;;; it, and run its body. The clause taken is the first that can go, in the
;;; order written; Go picks among the ready ones at random, for fairness,
;;; and Quokka chooses the order, so that a program runs the same way twice
;;; (step E's scheduler is deterministic too). With `otherwise`, nothing
;;; waits: if no clause can go at once, its body runs. A send clause on a
;;; closed channel is an error, as a `send` is.
(defmacro select (&rest clauses)
  (let ((arms nil) (otherwise nil))
    (dolist (clause clauses)
      (let ((head (car clause)))
        (cond ((and (symbolp head) (member (symbol-name head) '("OTHERWISE" "T") :test #'string=))
               (setq otherwise `(lambda () ,@(cdr clause))))
              ((and (consp head) (string= (symbol-name (car head)) "RECEIVE"))
               (destructuring-bind (channel var &optional (ok (gensym "OK"))) (cdr head)
                 (push `(list :receive ,channel
                              (lambda (,var ,ok) (declare (ignorable ,var ,ok)) ,@(cdr clause)))
                       arms)))
              ((and (consp head) (string= (symbol-name (car head)) "SEND"))
               (destructuring-bind (channel value) (cdr head)
                 (push `(list :send ,channel (lambda () ,@(cdr clause)) ,value) arms)))
              (t (error "~S is not a select clause." clause)))))
    `(%select (list ,@(nreverse arms)) ,otherwise)))

;;; Do what SELECT decided: run the body of ARM, the clause that went, with
;;; what it received.
(defun %run-arm (arm value ok)
  (if (eq (first arm) :receive)
      (multiple-value-bind (value ok) (%delivered value ok)
        (funcall (third arm) value ok))
      (if ok
          (funcall (third arm))
          (error 'closed-channel-error :channel (second arm)))))

;;; One critical section over every clause, in order: the first that can go
;;; goes, as (arm value ok); or, if none can and there is no `otherwise`, a
;;; waiter is put on every clause's channel, all in one selection, and the
;;; first partner to complete one completes the select. The bodies run
;;; outside the region in which the thread cannot be ended.
(defun %select (arms otherwise)
  (dolist (arm arms) (%check-channel (second arm)))
  (let ((selection (vector nil))
        (ready nil)
        (waited nil))
    (%without-interrupts
      (unwind-protect
           (progn
             (%with-channels
               (setq ready
                     (dolist (arm arms nil)
                       (let ((channel (second arm)))
                         (if (eq (first arm) :receive)
                             (let ((got (%try-receive channel)))
                               (when got (return (list arm (car got) (cdr got)))))
                             (let ((sent (%try-send channel (fourth arm))))
                               (when sent (return (list arm nil (eq sent :sent)))))))))
               (unless (or ready otherwise)
                 (dolist (arm arms)
                   (%queue-push (if (eq (first arm) :receive)
                                    (channel-receivers (second arm))
                                    (channel-senders (second arm)))
                                (%make-waiter (fourth arm) selection arm)))
                 (setq waited t)))
             (when waited (%wait-until (lambda () (svref selection 0)))))
        (when (and waited (not (%waiter-p (svref selection 0))))
          (%with-channels
            (unless (svref selection 0) (setf (svref selection 0) :cancelled))))))
    (cond (ready (%run-arm (first ready) (second ready) (third ready)))
          (waited (let ((fired (svref selection 0)))
                    (%run-arm (%waiter-clause fired) (%waiter-value fired) (%waiter-ok fired))))
          (t (funcall otherwise)))))

;;; ------------------------------------------------------------------------
;;; Tasks

;;; A task's error, carried to whoever receives its value.
(defstruct (%task-failure (:constructor %make-task-failure (condition))
                          (:copier nil))
  condition)

;;; Call FUNCTION with ARGUMENTS in a thread of its own, and return at once
;;; a channel that will receive its value, and is then closed, so that a
;;; second `receive` answers nil and nil instead of waiting for ever. An
;;; error in the task is not met in the task's thread, where nobody is
;;; waiting for it, but carried in the channel and signalled by `receive`,
;;; where the value was wanted, as Clojure's futures do; so is a task ended
;;; without a value, aborted or terminated, as a `thread-error`. FUNCTION
;;; may be a closure in one world; give it as a name to keep the task able
;;; to move to a worker world (#34, layer 2), where a closure cannot go.
(defun spawn-task (function &rest arguments)
  (let ((result (make-channel :capacity 1)))
    (make-thread
     (lambda ()
       (let ((sent nil))
         ;; The caller may have closed the channel; then nobody will read it,
         ;; and there is nothing to report.
         (unwind-protect
              (progn (handler-case
                         (send result (handler-case (apply function arguments)
                                        (error (condition) (%make-task-failure condition))))
                       (closed-channel-error () nil))
                     (setq sent t))
           (handler-case
               (progn
                 (unless sent
                   (try-send result (%make-task-failure
                                     (make-condition 'thread-error :thread (current-thread)))))
                 (close-channel result))
             (closed-channel-error () nil)))))
     :name (if (symbolp function)
               (format nil "task ~A" (string-downcase (symbol-name function)))
               "task"))
    result))
