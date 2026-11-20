;;; defmacro, backquote, lambda-list keywords, gensym.
;;;
;;; A macro is a function from code to code, run before evaluation. Everything
;;; here is behaviour the standard pins down exactly: what a template expands
;;; to, what &optional and &rest bind, and that two gensyms are never the same
;;; symbol. What a gensym is *called* is not pinned down, so nothing here
;;; prints one.

;; Backquote with no commas is just a quoted structure.
`(a b c)
`(a (b c) d)
`()
`(a . b)
`,(+ 2 2)

;; A comma evaluates one piece of the template.
(let ((x 1)) `(a ,x b))
(let ((x '(1 2))) `(a ,x b))

;; ,@ splices a list's elements in, rather than the list itself.
(let ((x '(1 2))) `(a ,@x b))
(let ((x '(1 2))) `(,@x))
(let ((x nil)) `(a ,@x b))
(let ((x '(1 2))) `(a ,@x))

;; The simplest useful macro: a one-armed if.
(defmacro my-when (test &body body)
  `(if ,test (progn ,@body) nil))

(my-when t 1 2 3)
(my-when nil 1 2 3)
(my-when (= 1 1) 'yes)

;; The body is not evaluated when the test fails — which is the whole point,
;; and the thing a function could not do.
(defun note (x) x)
(let ((seen nil))
  (my-when nil (setq seen 'ran))
  seen)

;; Macros see their arguments as code. This one never evaluates b at all.
(defmacro first-of (a b) `(note ,a))
(first-of 1 (car nil))

;; A macro can expand into another macro.
(defmacro my-unless (test &body body)
  `(my-when (null ,test) ,@body))
(my-unless nil 'ran)
(my-unless t 'ran)

;; &optional defaults to nil.
((lambda (a &optional b) (list a b)) 1)
((lambda (a &optional b) (list a b)) 1 2)

;; &rest collects what is left, as a list; nil when nothing is left.
((lambda (a &rest r) (list a r)) 1 2 3)
((lambda (a &rest r) (list a r)) 1)
((lambda (&rest r) r))

;; Variable capture, and the cure.
;;
;; This is `or` for two arguments, written the obvious way: bind the first
;; value so it is evaluated once, and return it if it is true. The bug is that
;; the macro picked the name V, and the second argument is then evaluated
;; *inside* that binding. A caller whose own variable is called V loses it.
(defmacro or2-badly (a b)
  `(let ((v ,a)) (if v v ,b)))

(let ((v 'outer)) (or2-badly nil v))
(let ((w 'outer)) (or2-badly nil w))

;; The same macro with a name the caller could not have written. gensym makes
;; a fresh, uninterned symbol: reading `G1` would produce a different symbol,
;; so nothing a caller types can collide with it.
(defmacro or2 (a b)
  (let ((tmp (gensym)))
    `(let ((,tmp ,a)) (if ,tmp ,tmp ,b))))

(let ((v 'outer)) (or2 nil v))
(let ((v 'outer)) (or2 'first v))
(or2 nil nil)

;; And the reason the binding is there at all: the first argument is evaluated
;; once, not twice.
(let ((n 0))
  (or2 (progn (setq n (+ n 1)) n) 'unused)
  n)

;; Two gensyms are two different symbols, and neither is interned.
(symbolp (gensym))
(eq (gensym) (gensym))

;; append, which backquote leans on, and which is worth testing in its own
;; right. The last argument is shared, not copied, so it need not be a list.
(append)
(append '(1 2))
(append '(1 2) '(3 4))
(append '(1) '(2) '(3) '(4))
(append nil '(1) nil)
(append '(1) 'x)
(append nil nil)
