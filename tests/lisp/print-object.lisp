;;; print-object, describe, with-slots (phase 3, CLOS sub-step 4, #124).
;;; Tier 1: compiled and SBCL only. SBCL prints an object's address where
;;; Quokka prints none, so no form prints an object with no method of its
;;; own, and `print-unreadable-object` is never given `:identity t`.
;;;
;;; scripts/check-differential.sh refuses `#<` in a baseline, because the
;;; `#<...>` an implementation writes for an object of its own never matches
;;; another's. Every one in this file is written by the file's own methods
;;; or by `print-unreadable-object`, whose output SBCL and Quokka agree on,
;;; so the forms that print one show their text through `shown`, with each
;;; `#<` turned into `#[`.
(defun shown (printed)
  (let ((text (copy-seq printed)))
    (dotimes (i (max 0 (- (length text) 1)) text)
      (when (and (char= (char text i) #\#) (char= (char text (+ i 1)) #\<))
        (setf (char text (+ i 1)) #\[)))))

(class-name (defclass point () ((x :initarg :x :accessor point-x)
                                (y :initarg :y :accessor point-y))))
(progn (defmethod print-object ((p point) stream)
         (print-unreadable-object (p stream :type t)
           (format stream "~S ~S" (point-x p) (point-y p))))
       :ok)

;; Every way of printing goes through the method.
(shown (prin1-to-string (make-instance 'point :x 1 :y 2)))
(shown (princ-to-string (make-instance 'point :x 1 :y 2)))
(shown (format nil "~A|~S" (make-instance 'point :x 1 :y 2) (make-instance 'point :x "a" :y 2)))
(shown (write-to-string (make-instance 'point :x 1 :y 2)))
(shown (prin1-to-string (list 1 (make-instance 'point :x 3 :y 4) (vector (make-instance 'point :x 5 :y 6)))))
(shown (with-output-to-string (s) (prin1 (make-instance 'point :x 1 :y 2) s)))
(shown (with-output-to-string (s) (princ (make-instance 'point :x "a" :y 2) s)))
(shown (with-output-to-string (s) (print (make-instance 'point :x 1 :y 2) s)))

;; A subclass wraps its parent's text with call-next-method.
(class-name (defclass point3 (point) ((z :initarg :z :accessor point-z))))
(progn (defmethod print-object ((p point3) stream)
         (write-string "[" stream)
         (call-next-method)
         (format stream " z=~S]" (point-z p)))
       :ok)
(shown (prin1-to-string (make-instance 'point3 :x 1 :y 2 :z 3)))

;; A method that writes with format alone.
(class-name (defclass tag () ((name :initarg :name :reader tag-name))))
(progn (defmethod print-object ((tg tag) stream) (format stream "<~A>" (tag-name tg))) :ok)
(prin1-to-string (list (make-instance 'tag :name "b") (make-instance 'tag :name 'i)))

;; A method's own printing continues the printout: *print-level* counts
;; from where the object was printed; neither the object nor
;; print-unreadable-object adds a level (SBCL, not pretty-printing).
(class-name (defclass box () ((contents :initarg :contents :reader contents))))
(progn (defmethod print-object ((b box) stream) (format stream "#<BOX ~S>" (contents b))) :ok)
(shown (let ((*print-level* 2)) (prin1-to-string (make-instance 'box :contents '(1 (2 (3)))))))
(shown (let ((*print-level* 2)) (prin1-to-string (list (make-instance 'box :contents '(1 (2 (3))))))))
(shown (let ((*print-level* 1)) (prin1-to-string (list (make-instance 'box :contents '(1 (2 (3))))))))
(shown (let ((*print-length* 2)) (prin1-to-string (list (make-instance 'box :contents '(1 2 3)) 2 3))))
(shown (let ((*print-level* 2)) (prin1-to-string (list (make-instance 'point :x '(1 (2)) :y 2)))))
(shown (let ((*print-level* 1)) (prin1-to-string (list (make-instance 'point :x 1 :y 2)))))

;; *print-circle* sees what a method prints.
(let ((*print-circle* t) (x (list 1 2)))
  (shown (prin1-to-string (list x (make-instance 'box :contents x)))))
(let ((*print-circle* t) (b (make-instance 'box :contents nil)))
  (setf (slot-value b 'contents) b)
  (shown (prin1-to-string b)))
(let ((*print-circle* t) (b (make-instance 'box :contents nil)))
  (setf (slot-value b 'contents) (list b))
  (shown (prin1-to-string (list b b))))
(let ((*print-circle* nil) (x (list 1 2)))
  (shown (prin1-to-string (list x (make-instance 'box :contents x)))))

;; Printing readably: print-unreadable-object signals.
(let ((*print-readably* t))
  (handler-case (prin1-to-string (make-instance 'point :x 1 :y 2))
    (print-not-readable () :not-readable)))
(let ((*print-readably* t))
  (handler-case (prin1-to-string (make-instance 'tag :name "b"))
    (print-not-readable () :not-readable)))

;; Structures: a method on a structure class, call-next-method to #S, and
;; the printer options of defstruct.
(defstruct pos x y)
(progn (defmethod print-object ((p pos) stream) (format stream "<POS ~A>" (pos-x p))) :ok)
(prin1-to-string (list (make-pos :x 1 :y 2)))
(defstruct (pos2 (:include pos)) z)
(prin1-to-string (make-pos2 :x 7 :y 8 :z 9))
(defstruct (pos3 (:include pos) (:print-function (lambda (p s d) d (format s "{POS3 ~A}" (pos-x p))))) z)
(prin1-to-string (make-pos3 :x 7))
(defstruct (pos4 (:include pos3)) w)
(prin1-to-string (make-pos4 :x 4))
(defstruct (plain (:print-object (lambda (p s) (format s "{PLAIN ~A}" (plain-a p))))) a)
(prin1-to-string (list (make-plain :a 1)))
(defstruct (plain2 (:include plain)) b)
(progn (defmethod print-object ((p plain2) stream) (write-string "{P2 " stream) (call-next-method) (write-string "}" stream)) :ok)
(prin1-to-string (make-plain2 :a 1 :b 2))
(defstruct bare a b)
(progn (defmethod print-object ((p bare) stream) (write-string "~" stream) (call-next-method)) :ok)
(prin1-to-string (make-bare :a 1 :b '(2)))
(let ((*print-level* 1)) (prin1-to-string (list (make-bare :a 1 :b '(2)))))
;; A structure redefined without its printer option keeps the method the
;; option made, as in SBCL.
(defstruct (again (:print-function (lambda (p s d) p d (write-string "AGAIN" s)))) a)
(prin1-to-string (make-again :a 1))
(defstruct again a)
(prin1-to-string (make-again :a 1))
(not (null (find-method #'print-object nil (list (find-class 'pos) (find-class t)) nil)))

;; print-unreadable-object on objects with no method: SBCL's spacing.
(shown (with-output-to-string (s) (print-unreadable-object (5 s))))
(shown (with-output-to-string (s) (print-unreadable-object (5 s) (write-string "x" s))))
(shown (with-output-to-string (s) (print-unreadable-object (5 s :type t))))
(shown (with-output-to-string (s) (print-unreadable-object (5 s :type t) (write-string "x" s))))
(shown (with-output-to-string (s) (print-unreadable-object ("ab" s :type nil) (write-string "x" s))))
(shown (with-output-to-string (s) (print-unreadable-object ((make-pos) s :type t) (write-string "x" s))))
(shown (with-output-to-string (s) (print-unreadable-object ((make-instance 'tag) s :type t))))
(shown (with-output-to-string (s) (print (print-unreadable-object ('(1 2) s :type t) (write-string "x" s)) s)))
(let ((*print-level* 1)) (with-output-to-string (s) (prin1 (list (make-instance 'tag :name "q")) s)))

;; describe calls describe-object, writes nothing of its own and returns
;; no values. Its default text is the implementation's (CLHS), so only a
;; program's own method is compared.
(progn (defmethod describe-object ((tg tag) stream) (format stream "TAG ~A" (tag-name tg))) :ok)
(with-output-to-string (s) (describe (make-instance 'tag :name "z") s))
(multiple-value-list (describe (make-instance 'tag :name "z") (make-string-output-stream)))
(multiple-value-list (describe 5 (make-string-output-stream)))
(let ((*standard-output* (make-string-output-stream)))
  (describe (make-instance 'tag :name "y") nil)
  (get-output-stream-string *standard-output*))
(handler-case (describe) (program-error () :program-error))

;; with-slots and with-accessors: slots and accessors as variables.
(let ((p (make-instance 'point :x 1 :y 2)))
  (with-slots (x y) p (list x y)))
(let ((p (make-instance 'point :x 1 :y 2)))
  (with-slots (x (why y)) p
    (setq x 10)
    (setf why 20)
    (incf x 5)
    (list x why (point-x p) (point-y p))))
(let ((p (make-instance 'point :x 1 :y 2)) (evaluations 0))
  (with-slots (x y) (progn (incf evaluations) p)
    (setq x (+ x y))
    (list x y evaluations)))
(let ((p (make-instance 'point :x 1 :y 2)))
  (with-accessors ((px point-x) (py point-y)) p
    (setf px (* py 7))
    (incf py)
    (list px py (point-x p) (point-y p))))
(let ((p (make-instance 'point :x 1 :y 2)) (q (make-instance 'point :x 3 :y 4)))
  (with-slots ((px x)) p
    (with-slots ((qx x)) q
      (rotatef px qx)
      (list (point-x p) (point-x q)))))
(let ((p (make-instance 'point :x 1 :y 2)))
  (with-slots (x) p
    (declare (ignorable x))
    (let ((x 99)) x)))
(let ((p (make-instance 'point :x 1 :y 2)))
  (with-slots (x) p (push :a x) (point-x p)))
(let ((s (make-bare :a 1 :b 2)))
  (with-slots (a b) s (setf a (+ a b)) (bare-a s)))
(let ((p (make-instance 'point :x 1 :y 2)))
  (with-accessors () p :empty))
(not (null (macro-function 'with-accessors)))

;; A method that prints its own object again, under *print-circle*.
(class-name (defclass selfish () ()))
(progn (defmethod print-object ((o selfish) s) (prin1 o s)) :ok)
(let ((*print-circle* t)) (prin1-to-string (make-instance 'selfish)))

;; A structure type printed plainly, then given a method, an eql method on
;; one instance, and a method on a type it includes.
(defstruct late a)
(prin1-to-string (list (make-late :a 1)))
(progn (defmethod print-object ((o late) s) (write-string "LATE" s)) :ok)
(prin1-to-string (list (make-late :a 1)))
(defvar *one-late* (make-late :a 2))
(progn (defmethod print-object ((o (eql *one-late*)) s) (write-string "ONE" s)) :ok)
(prin1-to-string (list *one-late* (make-late :a 3)))
(defstruct early a)
(defstruct (later (:include early)) b)
(prin1-to-string (make-later :a 1 :b 2))
(progn (defmethod print-object ((o early) s) (write-string "EARLY" s)) :ok)
(prin1-to-string (make-later :a 1 :b 2))

;; print-unreadable-object writes the type as princ or prin1 would.
(progn (defpackage :print-object-other (:use :cl)) :ok)
(class-name (defclass print-object-other::thing () ()))
(progn (defmethod print-object ((o print-object-other::thing) s)
         (print-unreadable-object (o s :type t) (write-string "x" s)))
       :ok)
(shown (princ-to-string (make-instance 'print-object-other::thing)))
(shown (prin1-to-string (make-instance 'print-object-other::thing)))
(shown (format nil "~A" (make-instance 'print-object-other::thing)))

;; A method that prints with :circle nil leaves the outer labels alone.
(class-name (defclass circle-off () ((v :initarg :v))))
(progn (defmethod print-object ((o circle-off) s)
         (write-string "<" s) (write (slot-value o 'v) :stream s :circle nil) (write-string ">" s))
       :ok)
(let* ((*print-circle* t) (x (list 1))) (prin1-to-string (list x (make-instance 'circle-off :v x))))
(let* ((*print-circle* t) (x (list 1))) (prin1-to-string (list (make-instance 'circle-off :v x) x)))

;; A defstruct printer named by a symbol is called by name when printing,
;; so it may be defined later, and redefined.
(defstruct (named-printer (:print-function print-named-printer)) a)
(defun print-named-printer (o s d) d (format s "NP~A" (named-printer-a o)))
(prin1-to-string (make-named-printer :a 1))
(defstruct (named-printer2 (:print-object print-named-printer2)) a)
(defun print-named-printer2 (o s) (format s "NP2-~A" (named-printer2-a o)))
(prin1-to-string (make-named-printer2 :a 1))
(defun print-named-printer2 (o s) (format s "NEW-~A" (named-printer2-a o)))
(prin1-to-string (make-named-printer2 :a 1))
