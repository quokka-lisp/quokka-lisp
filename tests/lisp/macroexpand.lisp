;;; apply, and seeing what a macro did.
;;;
;;; Every macro in this file is defined here rather than taken from the
;;; standard library, because two implementations are free to expand `when`
;;; or `cond` differently and both be right. What must agree is the expansion
;;; of a macro both of them were given.
;;;
;;; No macro here uses `gensym`: an expansion containing an uninterned symbol
;;; prints differently in every implementation, which `tests/lisp/README.md`
;;; rules out for good reason.

;; apply: the fixed arguments, then the elements of the last one.
(apply #'+ '(1 2 3))
(apply #'+ 1 2 '(3 4))
(apply #'+ '())
(apply #'list 1 2 '(3))
(apply #'list '(1))
(apply #'cons '(1 2))
(apply #'car '((1 2)))

;; The last argument may be empty, and may be the only one.
(apply #'list 1 2 3 '())
(apply #'+ 1 '())

;; apply reached through funcall, and funcall reached through apply.
(funcall #'apply #'+ '(1 2))
(apply #'funcall #'+ '(1 2))

;; One expansion.
(defmacro my-when (test &body body) `(if ,test (progn ,@body) nil))
(macroexpand-1 '(my-when t 1 2))
(macroexpand-1 '(my-when (= 1 1) 'yes))
(macroexpand '(my-when t 1 2))

;; A macro that expands into another macro: macroexpand-1 stops after one
;; step, macroexpand keeps going.
(defmacro outer (x) (list 'my-when t x))
(macroexpand-1 '(outer 5))
(macroexpand '(outer 5))

;; Three deep.
(defmacro outer2 (x) (list 'outer x))
(macroexpand-1 '(outer2 7))
(macroexpand '(outer2 7))

;; Things that are not macro calls come back unchanged.
(macroexpand-1 'foo)
(macroexpand-1 5)
(macroexpand-1 "text")
(macroexpand-1 '())
(macroexpand-1 '(car x))
(macroexpand-1 '(quote a))
(macroexpand-1 '(1 2 3))
(macroexpand '(car x))
(macroexpand 'foo)

;; Only the top of the form is expanded; subforms are left alone.
(macroexpand-1 '(list (my-when t 1)))
(macroexpand '(list (my-when t 1)))

;; A macro with a destructuring lambda list expands the same way.
(defmacro with-thing ((var name) &body body) `(let ((,var ,name)) ,@body))
(macroexpand-1 '(with-thing (h 'handle) h))

;; And one with &optional and &key.
(defmacro tagged (x &optional (tag 'none) &key (extra 'nothing))
  (list 'list x (list 'quote tag) (list 'quote extra)))
(macroexpand-1 '(tagged 1))
(macroexpand-1 '(tagged 1 here))
(macroexpand-1 '(tagged 1 here :extra more))

;; The expansion is an ordinary list, so it can be taken apart.
(car (macroexpand-1 '(my-when t 1)))
(length (macroexpand-1 '(my-when t 1)))
(macroexpand-1 (macroexpand-1 '(outer2 7)))
