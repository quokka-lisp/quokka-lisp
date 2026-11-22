;;;; setf expanders: get-setf-expansion, defsetf, define-setf-expander, and
;;;; the macros that read and write a place (#52).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;;
;;;; Most tests log the order in which subforms run: that every subform of a
;;;; place is evaluated once, left to right, is the whole point of the five
;;;; values. No test prints an expansion, which is the implementation's own.

(defvar *log* nil)
(defun note (tag value) (setq *log* (cons tag *log*)) value)
(defun drain () (let ((trail (reverse *log*))) (setq *log* nil) trail))

;; The prelude's places, written in the order the standard says: the place's
;; subforms first, then the value. (setf (car ...)) got this wrong until #52.
(let ((x (list 1 2)))
  (setf (car (note 'place x)) (note 'value 9))
  (list x (drain)))
(let ((x (list 1 2)))
  (setf (cdr (note 'place x)) (note 'value 9))
  (list x (drain)))
(let ((v (vector 1 2 3)))
  (setf (aref (note 'vector v) (note 'index 1)) (note 'value 20))
  (list v (drain)))

;; incf and decf: read, compute, write, each subform once.
(let ((x 5)) (incf x) x)
(let ((x 5)) (incf x 10))
(let ((x 5)) (decf x) (decf x 3) x)
(let ((v (vector 10 20 30)) (i 0))
  (incf (aref v (incf i)))
  (list v i))
(let ((v (vector 10 20 30)))
  (incf (aref (note 'vector v) (note 'index 2)) (note 'delta 5))
  (list v (drain)))
(let ((h (make-hash-table)))
  (incf (gethash 'a h 0))
  (incf (gethash 'a h 0) 4)
  (gethash 'a h))
(let ((x (list 1 2 3)))
  (incf (car (note 'cell x)))
  (list x (drain)))

;; push and pop. push evaluates its item before the place.
(let ((stack nil)) (push 1 stack) (push 2 stack) stack)
(let ((stack (list 1 2 3))) (list (pop stack) (pop stack) stack))
(let ((stack nil)) (pop stack))
(let ((v (vector nil nil)))
  (push (note 'item 'a) (aref (note 'vector v) (note 'index 1)))
  (list v (drain)))
(let ((h (make-hash-table)))
  (push 'x (gethash :k h))
  (push 'y (gethash :k h))
  (list (pop (gethash :k h)) (gethash :k h)))

;; pushnew adds only what is not there.
(let ((items (list 1 2))) (pushnew 2 items) (pushnew 3 items) items)
(let ((items (list "a" "b"))) (pushnew "a" items :test #'string=) items)
(let ((items (list (list 1 'one)))) (pushnew (list 1 'uno) items :key #'car) (length items))

;; rotatef, shiftf, psetf.
(let ((a 1) (b 2) (c 3)) (rotatef a b c) (list a b c))
(let ((v (vector 1 2 3))) (rotatef (aref v 0) (aref v 2)) v)
(let ((a 1) (b 2)) (list (shiftf a b 99) a b))
(let ((a 1) (b 2)) (psetf a b b a) (list a b))
(let ((a 1) (b 2)) (setf a b b a) (list a b))
(let ((x (list 1 2 3)))
  (rotatef (car (note 'first x)) (car (note 'last (cddr x))))
  (list x (drain)))
(let ((a 1)) (rotatef a) a)

;; The places decision 3 added.
(let ((x (list 1 2 3))) (setf (cadr x) 'two (caddr x) 'three) x)
(let ((x (list 1 2 3))) (setf (second x) 'b (third x) 'c) x)
(let ((x (list (list 1 2) 3))) (setf (caar x) 'a (cdar x) 'd) x)
(let ((x (list 1 2 3))) (setf (cddr x) '(z)) x)
(let ((x (list 1 2 3))) (setf (nth 2 x) 'n) (incf (nth 0 x) 10) x)
(let ((x (list 1 2 3)) (v (vector 1 2 3))) (setf (elt x 1) 'l (elt v 1) 'v) (list x v))
(defvar *counter* 0)
(progn (setf (symbol-value '*counter*) 41) (incf (symbol-value '*counter*)))
(let ((a 0) (b 0)) (setf (values a b) (values 1 2)) (list a b))
(let ((a 0) (b 0)) (list (multiple-value-list (setf (values a b) (values 3 4))) a b))
(let ((x 1)) (setf (the integer x) 5) (incf (the integer x)) x)

;; getf: a place inside a place. A new property goes on the front, so the
;; variable holding the list has to be written; an old one changes in place.
(let ((plist nil))
  (setf (getf plist :colour) 'red)
  (setf (getf plist :size) 3)
  (setf (getf plist :colour) 'blue)
  plist)
(let ((plist (list :a 1))) (incf (getf plist :a)) (incf (getf plist :b 10)) plist)
(getf '(:a 1 :b 2) :b)
(getf '(:a 1) :c 'none)
(let ((v (vector nil)))
  (setf (getf (aref v 0) :k) 'value)
  (push 'more (getf (aref v 0) :list))
  v)
(let ((plist (list :a 1 :b 2 :c 3)))
  (list (remf plist :b) plist (remf plist :a) plist (remf plist :z) plist))

;; defsetf, short form.
(defun my-first (list) (car list))
(defun set-my-first (list value) (rplaca list value) value)
(defsetf my-first set-my-first)
(let ((x (list 1 2))) (list (setf (my-first x) 10) (incf (my-first x)) x))

;; defsetf, long form: the body builds the writer from temporaries.
(defun find-pair (key alist)
  (cond ((null alist) nil) ((eq (car (car alist)) key) (car alist)) (t (find-pair key (cdr alist)))))
(defun lookup (key alist) (cdr (find-pair key alist)))
(defun update (key alist value)
  (let ((pair (find-pair key alist)))
    (if pair (rplacd pair value) (error "no such key"))
    value))
(defsetf lookup (key alist) (value) `(update ,key ,alist ,value))
(let ((table (list (cons 'a 1) (cons 'b 2))))
  (setf (lookup 'a table) 100)
  (incf (lookup (note 'key 'b) (note 'table table)) (note 'delta 5))
  (list table (drain)))

;; define-setf-expander, written out in full: a place that keeps a counter.
(defvar *writes* 0)
(defun cell-value (cell) (car cell))
(define-setf-expander cell-value (cell)
  (let ((temp (gensym)) (store (gensym)))
    (values (list temp) (list cell) (list store)
            `(progn (setq *writes* (+ *writes* 1)) (rplaca ,temp ,store) ,store)
            `(car ,temp))))
(let ((c (list 0)))
  (setf (cell-value c) 5)
  (incf (cell-value c))
  (push 'x (cell-value c))
  (list c *writes*))

;; A macro form is a place when what it expands to is one.
(defmacro head-of (x) `(car ,x))
(let ((x (list 1 2))) (setf (head-of x) 'h) (incf (cadr x)) x)

;; define-modify-macro.
(define-modify-macro appendf (&rest lists) append)
(let ((x (list 1))) (appendf x (list 2) (list 3)) x)
(define-modify-macro multiplyf (factor) *)
(let ((v (vector 2 3))) (multiplyf (aref v 1) 7) v)

;; get-setf-expansion's shape, without printing anything implementation-made.
(multiple-value-bind (vars vals stores writer reader) (get-setf-expansion 'x)
  (list vars vals (length stores) (consp writer) reader))
(multiple-value-bind (vars vals stores) (get-setf-expansion '(aref v (f i)))
  (list (length vars) vals (length stores)))
