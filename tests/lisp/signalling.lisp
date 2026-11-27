;;; Signalling in Lisp (phase 3 step B, sub-step 2, #125): handler-bind,
;;; signal and error, and how handler-case and handler-bind nest.

(define-condition sg-note (condition) ((n :initarg :n :initform 0 :reader sg-note-n)))
(define-condition sg-error (error) ())
(define-condition sg-sub-error (sg-error) ())

;; A handler runs where the condition is signalled, and may decline.
(let ((seen nil))
  (list (handler-bind ((sg-note (lambda (c) (push (sg-note-n c) seen))))
          (signal 'sg-note :n 1)
          (signal 'sg-note :n 2)
          :done)
        seen))
(signal 'sg-note :n 3)
(block out
  (handler-bind ((sg-error (lambda (c) (return-from out (list :caught (type-of c))))))
    (error 'sg-sub-error)))

;; Inner clusters first; in a cluster, every handler that applies, in order.
(let ((trail nil))
  (handler-bind ((condition (lambda (c) c (push :outer trail))))
    (handler-bind ((condition (lambda (c) c (push :inner trail))))
      (signal 'sg-note)))
  (nreverse trail))
(let ((trail nil))
  (handler-bind ((sg-note (lambda (c) c (push 1 trail)))
                 (error (lambda (c) c (push 2 trail)))
                 (condition (lambda (c) c (push 3 trail))))
    (signal 'sg-note))
  (nreverse trail))

;; While a handler runs, only the handlers outside its cluster are active.
(let ((trail nil))
  (handler-bind ((sg-note (lambda (c) c (push :outer trail))))
    (handler-bind ((sg-note (lambda (c) c (push :inner trail) (signal 'sg-note))))
      (signal 'sg-note)))
  (nreverse trail))
(let ((trail nil))
  (handler-bind ((sg-note (lambda (c) c (push :outer trail))))
    (handler-bind ((sg-note (lambda (c) c
                              (handler-bind ((sg-note (lambda (c) c (push :own trail))))
                                (signal 'sg-note))
                              (push :inner trail))))
      (signal 'sg-note)))
  (nreverse trail))

;; handler-case and handler-bind nest in the order they were established.
(let ((trail nil))
  (list (handler-case
            (handler-bind ((error (lambda (c) c (push :bind trail))))
              (error "x"))
          (error () (push :case trail) :case-value))
        (nreverse trail)))
(let ((trail nil))
  (handler-bind ((error (lambda (c) c (push :outer-bind trail))))
    (handler-case (error "x") (error () (push :case trail))))
  (nreverse trail))
(let ((trail nil))
  (handler-case
      (handler-bind ((sg-error (lambda (c) c (push :bind trail))))
        (handler-case (error 'sg-sub-error)
          (type-error () (push :wrong-clause trail))))
    (sg-error (c) (push (type-of c) trail)))
  (nreverse trail))

;; error and signal: a format control, a condition type with initargs, a
;; condition.
(handler-case (error "a ~A b ~S" 1 "c") (simple-error (c) (princ-to-string c)))
(handler-case (error 'type-error :datum 3 :expected-type 'list)
  (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (error (make-condition 'sg-error)) (sg-error (c) (type-of c)))
(handler-case (signal "s ~A" 2)
  (simple-condition (c) (list (type-of c) (princ-to-string c))))
(handler-case (error 'sg-error) ((and error (not type-error)) () :compound))
(handler-case (error 'sg-error) ((or type-error sg-error) () :either))

;; A condition that is not an error passes an error clause by.
(handler-case (progn (signal 'sg-note) :not-caught) (error () :caught))
(handler-case (signal 'sg-note) (condition (c) (type-of c)))
(handler-case (signal 'sg-note) (t () :t-clause))

;; A handler named by a symbol or given as a function.
(defun sg-handler (c) (throw :sg (type-of c)))
(catch :sg (handler-bind ((sg-error #'sg-handler)) (error 'sg-error)))
(catch :sg (handler-bind ((sg-error 'sg-handler)) (error 'sg-error)))

;; Unwinding to a handler-case runs cleanups and restores special variables.
(let ((trail nil))
  (handler-case (unwind-protect (error "x") (push :cleanup trail))
    (error () (push :handled trail)))
  (nreverse trail))
(defvar *sg-v* 1)
(handler-case (let ((*sg-v* 2)) (error "x")) (error () *sg-v*))
(handler-bind ((error (lambda (c) c (throw :sg *sg-v*))))
  (catch :sg (let ((*sg-v* 3)) (error "y"))))

;; The machine's own errors still reach handler-case.
(handler-case (car 5) (type-error () :type-error))
(multiple-value-bind (value condition) (ignore-errors (error 'sg-error))
  (list value (type-of condition)))

;; An error a handler declines, and no other takes.
(handler-bind ((sg-error (lambda (c) c nil))) (error 'sg-error))
