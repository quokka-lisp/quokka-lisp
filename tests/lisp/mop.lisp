;;; MOP introspection (phase 3, CLOS sub-step 7, #124): the introspective
;;; half of the metaobject protocol, in QUOKKA-MOP as SBCL's is in SB-MOP.
;;; Tier 1: compiled and SBCL only. The file runs in a package that uses
;;; whichever MOP package the Lisp has.

(progn (defpackage :mop-user (:use :common-lisp #+sbcl :sb-mop #+quokka :quokka-mop)) :ok)
(progn (in-package :mop-user) :ok)

;; A class precedence list without the implementation's own classes.
(defun standard-names (classes)
  (remove-if (lambda (name)
               (eq (symbol-package name) (find-package #+sbcl :sb-pcl #+quokka :quokka)))
             (mapcar #'class-name classes)))

(class-name (defclass mp () ((a :initarg :a :initform 1 :reader mp-a :type integer)
                             (b :allocation :class :initarg :b :initform (+ 1 2)))))
(class-name (defclass mq (mp) ((c :writer set-mq-c :accessor mq-c :documentation "c"))
              (:default-initargs :a 5)))

;; Classes.
(mapcar #'class-name (class-direct-superclasses (find-class 'mq)))
(progn (make-instance 'mq) (class-finalized-p (find-class 'mq)))
(standard-names (class-precedence-list (find-class 'mq)))
(mapcar #'slot-definition-name (class-direct-slots (find-class 'mq)))
(mapcar #'slot-definition-name (class-slots (find-class 'mq)))
(not (null (member (find-class 'mq) (class-direct-subclasses (find-class 'mp)))))
(mapcar (lambda (i) (list (first i) (second i))) (class-direct-default-initargs (find-class 'mq)))
(mapcar (lambda (i) (list (first i) (second i) (funcall (third i)))) (class-default-initargs (find-class 'mq)))
(typep (class-prototype (find-class 'mq)) 'mq)
(progn (finalize-inheritance (find-class 'mq)) (class-finalized-p (find-class 'mq)))
(standard-names (class-precedence-list (find-class 'standard-generic-function)))

;; Slot definitions.
(let ((a (first (class-direct-slots (find-class 'mp)))))
  (list (slot-definition-name a) (slot-definition-initargs a) (slot-definition-initform a)
        (funcall (slot-definition-initfunction a)) (slot-definition-allocation a)
        (slot-definition-type a) (slot-definition-readers a) (slot-definition-writers a)))
(let ((c (first (class-direct-slots (find-class 'mq)))))
  (list (slot-definition-readers c) (slot-definition-writers c)))
(let ((b (second (class-direct-slots (find-class 'mp)))))
  (list (slot-definition-allocation b) (slot-definition-initform b) (funcall (slot-definition-initfunction b))))
(mapcar #'slot-definition-allocation (class-slots (find-class 'mq)))
(mapcar (lambda (s) (typep s 'standard-effective-slot-definition)) (class-slots (find-class 'mq)))
(mapcar (lambda (s) (typep s 'standard-direct-slot-definition)) (class-direct-slots (find-class 'mq)))
(let* ((class (find-class 'mq))
       (instance (make-instance 'mq :a 7))
       (a (find 'a (class-slots class) :key #'slot-definition-name)))
  (list (integerp (slot-definition-location a))
        (standard-instance-access instance (slot-definition-location a))))
(let ((b (find 'b (class-slots (find-class 'mq)) :key #'slot-definition-name)))
  (consp (slot-definition-location b)))

;; Generic functions and methods.
(progn (defgeneric mg (x y &optional z) (:argument-precedence-order y x)) :ok)
(progn (defmethod mg ((x mp) (y integer) &optional z) z)
       (defmethod mg :before ((x mq) y &optional z) z)
       (defmethod mg ((x (eql 5)) y &optional z) z)
       :ok)
(generic-function-name #'mg)
(generic-function-lambda-list #'mg)
(generic-function-argument-precedence-order #'mg)
(length (generic-function-methods #'mg))
(typep (generic-function-method-combination #'mg) 'method-combination)
(eq (generic-function-method-class #'mg) (find-class 'standard-method))
(let ((m (find-method #'mg '(:before) '(mq t))))
  (list (method-qualifiers m) (mapcar #'class-name (method-specializers m))
        (eq (method-generic-function m) #'mg) (method-lambda-list m)
        (functionp (method-function m))))
(let ((m (find-method #'mg '() (list (intern-eql-specializer 5) (find-class 't)))))
  (eql-specializer-object (first (method-specializers m))))
(eq (intern-eql-specializer 'x) (intern-eql-specializer 'x))
(typep (intern-eql-specializer 1) 'eql-specializer)
(length (specializer-direct-methods (find-class 'mq)))
(let ((names (mapcar #'generic-function-name (specializer-direct-generic-functions (find-class 'mq)))))
  (list (length names) (and (member 'mg names) (member 'mq-c names) (member '(setf mq-c) names :test #'equal)
                            (member 'set-mq-c names) t)))
(length (specializer-direct-methods (intern-eql-specializer 5)))
(let ((m (find-method #'mp-a '() '(mp))))
  (list (typep m 'standard-reader-method) (slot-definition-name (accessor-method-slot-definition m))))
(let ((m (find-method #'set-mq-c '() '(t mq))))
  (list (typep m 'standard-writer-method) (slot-definition-name (accessor-method-slot-definition m))))
(typep (find-method-combination #'mg 'standard nil) 'method-combination)
(extract-lambda-list '((x mp) (y (eql 3)) &optional (z 1) &key k))
(extract-specializer-names '((x mp) (y (eql 3)) z &rest more))

;; Funcallable instances.
(class-name (defclass mf () ((n :initarg :n)) (:metaclass funcallable-standard-class)))
(let ((f (make-instance 'mf :n 3)))
  (set-funcallable-instance-function f (lambda (x) (* x (slot-value f 'n))))
  (list (funcall f 4) (functionp f) (typep f 'funcallable-standard-object)))
(standard-names (class-precedence-list (find-class 'mf)))

;; The metaobject classes.
(standard-names (class-precedence-list (find-class 'standard-effective-slot-definition)))
(standard-names (class-precedence-list (find-class 'eql-specializer)))
(standard-names (class-precedence-list (find-class 'standard-reader-method)))

;; A slot's readers and writers are listed newest first, its initargs in order.
(class-name (defclass mr () ((c :reader r1 :accessor r2 :reader r3 :writer w1 :writer w2
                                :initarg :a :initarg :b :initarg :a))))
(let ((c (first (class-direct-slots (find-class 'mr)))))
  (list (slot-definition-readers c) (slot-definition-writers c) (slot-definition-initargs c)))
