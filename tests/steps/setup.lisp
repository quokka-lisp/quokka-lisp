;;;; What the step benchmark's forms are measured against (#169): a few
;;;; classes, generic functions and objects. Each form in `forms.txt` is
;;;; put into a function that evaluates it a thousand times; the steps
;;;; that takes, less the steps of an empty loop, is the form's cost.
;;;; A name bound here is read by those forms: change one and the counts
;;;; in `baseline.txt` change with it.

(defclass a () ((x :initform 1 :accessor ax)))
(defclass big ()
  ((s0 :initform 0) (s1 :initform 0) (s2 :initform 0) (s3 :initform 0) (s4 :initform 0)
   (s5 :initform 0) (s6 :initform 0) (s7 :initform 0) (s8 :initform 0)
   (s9 :initform 0 :accessor s9)))
(defstruct pt x)

(defun plain (o) o)
(defgeneric g0 ())
(defmethod g0 () 1)
(defgeneric g1 (o))
(defmethod g1 ((o a)) 1)
(defmethod g1 ((o integer)) 2)
(defmethod g1 ((o pt)) 3)
(defgeneric g2 (o p))
(defmethod g2 ((o a) (p a)) 1)
(defgeneric ge (o))
(defmethod ge ((o (eql :k))) 1)
(defmethod ge ((o a)) 2)
(defgeneric around (o))
(defmethod around ((o a)) 1)
(defmethod around :around ((o a)) (call-next-method))
(defmethod around :before ((o a)) nil)
(defgeneric poly (o))
(defmethod poly ((o t)) 1)
(defclass c0 () ()) (defclass c1 () ()) (defclass c2 () ()) (defclass c3 () ())
(defclass c4 () ()) (defclass c5 () ()) (defclass c6 () ()) (defclass c7 () ())
(defclass c8 () ()) (defclass c9 () ())
(define-condition bench-error (error) ())

(defvar *a* (make-instance 'a))
(defvar *big* (make-instance 'big))
(defvar *p* (make-pt :x 1))
(defvar *v* (vector (make-instance 'c0) (make-instance 'c1) (make-instance 'c2)
                    (make-instance 'c3) (make-instance 'c4) (make-instance 'c5)
                    (make-instance 'c6) (make-instance 'c7) (make-instance 'c8)
                    (make-instance 'c9)))
(defvar *list* (list 1 2 3 4 5 6 7 8 9 10))
(defvar *string* "0123456789012345678901234567890123456789")
(defvar *text* (make-instance 'quokka-ui:text-record))
