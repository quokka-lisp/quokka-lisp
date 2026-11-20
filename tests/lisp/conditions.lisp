;;; error, handler-case and ignore-errors.
;;;
;;; Only conditions **Common Lisp already defines** appear here. Quokka can
;;; signal a condition named by any symbol — `(error 'boom)` — but in a real
;;; Common Lisp that symbol has to name a condition class defined with
;;; `define-condition`, which Quokka does not have. So the portable subset is
;;; `(error "text")`, which makes a `simple-error`, and the condition types
;;; the evaluator itself signals when a program does something wrong.
;;;
;;; That is the interesting half anyway: the point of group E is that an
;;; *evaluator* error stops being a Rust value that halts the machine and
;;; becomes something Lisp code can catch.

;; A form that does not signal is returned untouched.
(handler-case 42 (error () 'not-reached))
(handler-case (+ 1 2) (error () 'not-reached))
(handler-case (list 1 2) (error () 'not-reached))

;; An explicit error, caught.
(handler-case (error "something went wrong") (error () 'caught))
(handler-case (error "something went wrong") (simple-error () 'simple))

;; The condition is bound when the clause asks for it, and need not be.
(handler-case (error "x") (error (c) 'bound))
(handler-case (error "x") (error () 'unbound))

;; Errors the evaluator itself signals are ordinary conditions now.
(handler-case (car 5) (error () 'caught))
(handler-case (car 5) (type-error () 'type-error))
(handler-case (no-such-function-anywhere) (undefined-function () 'undefined))
(handler-case (aref (vector 1) 9) (error () 'out-of-bounds))

;; Clauses are tried in order, and the first matching type wins.
(handler-case (error "x") (error () 'first) (simple-error () 'second))
(handler-case (car 5) (type-error () 'specific) (error () 'general))

;; A clause naming t catches anything.
(handler-case (error "x") (t () 'any))

;; Nothing after the signal runs.
(handler-case (progn (error "x") 'unreached) (error () 'stopped))
(handler-case (list 1 (error "x") 3) (error () 'stopped))

;; The handler's value is the value of the whole form.
(+ 1 (handler-case (error "x") (error () 10)))
(list (handler-case 1 (error () 'no)) (handler-case (error "x") (error () 2)))

;; Nested: the innermost handler that wants it gets it.
(handler-case (handler-case (error "x") (type-error () 'inner-wrong))
  (error () 'outer-caught))
(handler-case (handler-case (error "x") (error () 'inner-caught))
  (error () 'outer-wrong))

;; unwind-protect cleanups run while a condition unwinds past them.
(let ((cleaned nil))
  (list (handler-case (unwind-protect (error "x") (setq cleaned 'ran))
          (error () 'caught))
        cleaned))

;; Cleanups run innermost first, as they do for any unwind.
(let ((order nil))
  (handler-case
      (unwind-protect
           (unwind-protect (error "x") (setq order (cons 'inner order)))
        (setq order (cons 'outer order)))
    (error () nil))
  order)

;; A handler-case is transparent to block and catch passing through it.
(block out (handler-case (return-from out 'returned) (error () 'not-reached)))
(catch 'tag (handler-case (throw 'tag 'thrown) (error () 'not-reached)))

;; And a signal passes through a block on its way out.
(handler-case (block inner (error "x")) (error () 'caught))

;; ignore-errors.
(ignore-errors 42)
(ignore-errors (+ 1 2))
(ignore-errors (error "x"))
(ignore-errors (car 5))
(list (ignore-errors (error "x")) (ignore-errors 'fine))

;; A handler can itself signal, and an outer handler catches that.
(handler-case (handler-case (error "inner") (error () (error "from the handler")))
  (error () 'outer-caught))

;; A function that signals, caught at the call site.
(defun risky (n) (if (= n 0) (error "zero") n))
(handler-case (risky 5) (error () 'caught))
(handler-case (risky 0) (error () 'caught))
(list (ignore-errors (risky 0)) (ignore-errors (risky 7)))

;; Signalling out of a deep recursion unwinds all of it.
(defun countdown (n) (if (= n 0) (error "bottom") (countdown (- n 1))))
(handler-case (countdown 100) (error () 'caught-from-depth))
