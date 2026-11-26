;;; What ansi-test's objects group found (phase 3, CLOS sub-step 7, #124):
;;; accessors called with too many arguments, repeated options, proper
;;; names, class-name as a generic function, the values slot-missing and
;;; slot-unbound give, make-load-form. Tier 1: compiled and SBCL only.

(defun condition-kind (c)
  (cond ((typep c 'program-error) :program-error)
        ((typep c 'simple-error) :simple-error)
        (t :error)))

;; A reader or writer checks how many arguments it was given, after
;; dispatch.
(class-name (defclass dacc () ((s1 :reader d-r :writer d-w :accessor d-a))))
(handler-case (d-r (make-instance 'dacc) 1) (error (c) (condition-kind c)))
(handler-case (d-w 1 (make-instance 'dacc) 2) (error (c) (condition-kind c)))
(handler-case (funcall #'(setf d-a) 1 (make-instance 'dacc) 2) (error (c) (condition-kind c)))
(handler-case (d-a) (error (c) (condition-kind c)))
(handler-case (d-w (make-instance 'dacc) 'a 'b) (error (c) (condition-kind c)))

;; Options given twice.
(mapcar (lambda (options)
          (handler-case (progn (eval `(defgeneric ,(gensym) (x) ,@options)) :ok)
            (error (c) (condition-kind c))))
        '(((:documentation "a") (:documentation "b"))
                   ((:method-combination +) (:method-combination list))
                   ((:generic-function-class standard-generic-function)
                    (:generic-function-class standard-generic-function))
                   ((:method-class standard-method) (:method-class standard-method))
                   ((:argument-precedence-order x) (:argument-precedence-order x))
                   ((declare (optimize speed)) (declare (optimize safety)))))
(mapcar (lambda (options)
          (handler-case (progn (eval `(defclass ,(gensym) () ((s :initarg :a)) ,@options)) :ok)
            (error (c) (condition-kind c))))
        '(((:default-initargs :a 1 :a 2))
                   ((:default-initargs :a 1) (:default-initargs :b 2))
                   ((:documentation "a") (:documentation "b"))
                   ((:metaclass standard-class) (:metaclass standard-class))
                   ((:default-initargs :a 1 :b 2))))

;; class-name and (setf class-name) are generic functions.
(list (typep #'class-name 'generic-function) (typep #'(setf class-name) 'generic-function))
(class-name (defclass dcn () ()))
(progn (defmethod class-name ((x dcn)) 'silly) :ok)
(class-name (make-instance 'dcn))
(multiple-value-list (setf (class-name (find-class 'dcn)) 'dcn2))
(list (class-name (find-class 'dcn)) (find-class 'dcn2 nil))

;; A class that has lost its proper name is not redefined by defclass: a
;; new class is made.
(class-name (defclass dpn () ((a))))
(let ((old (find-class 'dpn))
      (instance (make-instance 'dpn)))
  (setf (class-name old) nil)
  (let ((new (eval '(defclass dpn () ((a))))))
    (list (eq old new) (class-name old) (class-name new) (eq (class-of instance) old)
          (typep instance new) (eq (find-class 'dpn) new))))
(class-name (defclass dpn2 () ((a))))
(let ((old (find-class 'dpn2)))
  (setf (class-name old) 'dpn3)
  (let ((new (eval '(defclass dpn2 () ((a))))))
    (list (eq old new) (class-name old) (class-name new))))

;; slot-boundp gives the truth of what slot-missing returns; slot-value and
;; a reader give the first value of slot-missing and slot-unbound.
(class-name (defclass dmiss () ((a :reader dmiss-a))))
(progn (defmethod slot-missing ((class t) (object dmiss) name operation &optional value)
         value
         (values (list name operation) 2))
       :ok)
(list (slot-boundp (make-instance 'dmiss) 'zz))
(multiple-value-list (slot-value (make-instance 'dmiss) 'zz))
(progn (defmethod slot-unbound ((class t) (object dmiss) name) (values name 2 3)) :ok)
(multiple-value-list (slot-value (make-instance 'dmiss) 'a))
(multiple-value-list (dmiss-a (make-instance 'dmiss)))
(progn (defmethod slot-unbound ((class t) (object dmiss) (name (eql 'a))) (values)) :ok)
(multiple-value-list (slot-value (make-instance 'dmiss) 'a))

;; make-load-form and make-load-form-saving-slots.
(typep #'make-load-form 'generic-function)
(class-name (defclass dmlf () ((a :initarg :a) (b :initarg :b) (c :initarg :c :allocation :class))))
(handler-case (make-load-form (make-instance 'dmlf)) (error (c) (condition-kind c)))
(defstruct dmls a (b 2 :read-only t))
(handler-case (make-load-form (make-dmls)) (error (c) (condition-kind c)))
(handler-case (make-load-form (handler-case (error "x") (error (c) c))) (error (c) (condition-kind c)))
(handler-case (make-load-form 5) (error (c) (condition-kind c)))
(make-load-form (find-class 'dmlf))
(let ((object (make-instance 'dmlf :a 1)))
  (multiple-value-bind (create initialize) (make-load-form-saving-slots object)
    (list create (subst :object object initialize))))
(let ((object (make-instance 'dmlf :a 1)))
  (multiple-value-bind (create initialize) (make-load-form-saving-slots object :slot-names '(b))
    (list create (subst :object object initialize))))
(let* ((object (make-instance 'dmlf :a '(x y) :b 7))
       (forms (multiple-value-list (make-load-form-saving-slots object :environment nil)))
       (copy (eval (first forms))))
  (eval (subst copy object (second forms)))
  (list (length forms) (eq (class-of copy) (class-of object)) (slot-value copy 'a) (slot-value copy 'b)))
(let* ((object (make-dmls :a 17 :b 5))
       (forms (multiple-value-list (make-load-form-saving-slots object)))
       (copy (eval (first forms))))
  (eval (subst copy object (second forms)))
  (list (length forms) (dmls-p copy) (dmls-a copy) (dmls-b copy)))
(handler-case (make-load-form-saving-slots) (error (c) (condition-kind c)))
(handler-case (make-load-form-saving-slots (make-instance 'dmlf) :slot-names) (error (c) (condition-kind c)))

;; allocate-instance on a structure class.
(let ((object (allocate-instance (find-class 'dmls))))
  (setf (dmls-a object) 'x)
  (list (dmls-p object) (typep object 'dmls) (eq (class-of object) (find-class 'dmls)) (dmls-a object)))

;; A built-in class cannot be renamed, as in SBCL.
(handler-case (setf (class-name (find-class 'integer)) 'whole-number) (error (c) (condition-kind c)))
(class-name (find-class 'integer))

;; A structure class can be renamed. It then has no proper name, and a
;; `defclass` of the old name makes a new class; the structures keep theirs.
(defstruct drs (a 1))
(defvar *drs-class* (find-class 'drs))
(setf (class-name *drs-class*) 'drs-renamed)
(list (eq (class-of (make-drs)) *drs-class*) (class-name (class-of (make-drs))))
(progn (defclass drs () ((z :initform 3))) :defined)
(list (eq (find-class 'drs) *drs-class*)
      (eq (class-of (make-drs)) *drs-class*)
      (class-name (class-of (make-drs)))
      (slot-value (make-instance 'drs) 'z)
      (drs-a (make-drs)))

;; allocate-instance finds a structure's layout from its class, whatever
;; the class is now called.
(defstruct dra (b 2))
(let ((rc (find-class 'dra)))
  (setf (class-name rc) 'dra-renamed)
  (let ((object (allocate-instance rc)))
    (list (typep object rc) (eq (class-of object) rc))))

;; make-instance of a structure class: the slots' initforms, then
;; initialize-instance; a structure's slots take no initargs.
(defvar *dmi-count* 0)
(defstruct dmi (a (incf *dmi-count*)) b)
(defstruct (dmi2 (:include dmi)) (c 3))
(progn (defmethod initialize-instance :after ((s dmi) &key) (setf (dmi-b s) :init)) :defined)
(make-instance 'dmi)
(make-instance (find-class 'dmi2) :allow-other-keys t :a 5)
(handler-case (make-instance 'dmi :a 5) (error (c) (condition-kind c)))
