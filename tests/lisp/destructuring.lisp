;;; Destructuring macro lambda lists.
;;;
;;; A macro receives its arguments as *forms* — lists — so its parameter list
;;; may describe a shape rather than a sequence of names. This is what makes
;;; `(with-open-file (stream "x") ...)` possible, and it is the reason macros
;;; are worth having beyond clever control flow.
;;;
;;; Pattern arity is checked now, so a mis-shaped call signals rather than
;;; quietly binding nil — which it did until there was a condition system to
;;; signal into. Both the well-shaped and the mis-shaped cases are here.

;; The simplest pattern, and the one chapter 17 is built around.
(defmacro with-thing ((var name) &body body)
  `(let ((,var ,name)) ,@body))

(with-thing (h 'handle) h)
(with-thing (h 'handle) (list h h))
(with-thing (h (+ 1 2)) (* h 10))

;; More than one pattern, side by side.
(defmacro pair ((a b) (c d)) `(list ,a ,b ,c ,d))
(pair (1 2) (3 4))
(pair ('x 'y) ('z 'w))

;; Patterns nest.
(defmacro deep ((a (b c)) d) `(list ,a ,b ,c ,d))
(deep (1 (2 3)) 4)
(defmacro deeper (((a b) (c (d e)))) `(list ,a ,b ,c ,d ,e))
(deeper ((1 2) (3 (4 5))))

;; A pattern mixed with ordinary parameters.
(defmacro mixed (x (a b) y) `(list ,x ,a ,b ,y))
(mixed 1 (2 3) 4)

;; &optional inside a pattern, with and without a default.
(defmacro withopt ((var name &optional (mode 'read)) &body body)
  `(list ,var ,name ',mode ,@body))
(withopt (1 2) 9)
(withopt (1 2 write) 9)

(defmacro plainopt ((a &optional b)) `(list ,a ',b))
(plainopt (1))
(plainopt (1 2))

;; supplied-p inside a pattern.
(defmacro sup ((a &optional (b 0 b-p))) `(list ,a ,b ',b-p))
(sup (1))
(sup (1 5))

;; &rest inside a pattern.
(defmacro restin ((a &rest r)) `(list ,a ',r))
(restin (1))
(restin (1 2 3))

;; &body inside a pattern means the same thing.
(defmacro bodyin ((a &body r)) `(list ,a ',r))
(bodyin (1 2 3))

;; A dotted pattern tail.
(defmacro dottedpat ((a . r)) `(list ,a ',r))
(dottedpat (1 2 3))

;; A dotted macro lambda list: the tail is &rest by another spelling.
(defmacro dotted (a . b) `(list ,a ',b))
(dotted 1 2 3)
(dotted 1)

;; The real thing: a with- macro that cleans up, including when the body
;; leaves early. This is chapter 17's example, run rather than described.
(defun open-thing (name) (list 'open name))
(defun close-thing (h) (setq closed h))

(defmacro with-open-thing ((var name) &body body)
  `(let ((,var (open-thing ,name)))
     (unwind-protect (progn ,@body)
       (close-thing ,var))))

(setq closed nil)
(with-open-thing (handle 'x) (list 'used handle))
closed

(setq closed nil)
(catch 'oops
  (with-open-thing (handle 'y)
    (throw 'oops 'left-early)))
closed

;; Patterns and the top-level lambda-list features together.
(defmacro combo ((a b) &optional (c 'dflt) &key (d 'keyed))
  `(list ,a ,b ',c ',d))
(combo (1 2))
(combo (1 2) three)
(combo (1 2) three :d four)

;; A call that does not fit its pattern signals, rather than binding nil for
;; what is missing or ignoring what is left over. Written as top-level forms
;; because the failure happens while the call is being *expanded*, and an
;; implementation is free to expand it before a handler around it is in
;; force — so `!! error` from both is the portable thing to compare.
(defmacro exactly-two ((a b)) (list 'quote (list a b)))
(exactly-two (1 2))
(exactly-two (1))
(exactly-two (1 2 3))
(exactly-two ())

;; &optional and &rest still accept what they are meant to, and &rest makes
;; a long call legal again.
(defmacro opt ((a &optional b)) (list 'quote (list a b)))
(opt (1))
(opt (1 2))
(defmacro res ((a &rest r)) (list 'quote (list a r)))
(res (1))
(res (1 2 3))
