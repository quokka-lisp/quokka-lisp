;;; Classes (phase 3, step A2, #124): the class hierarchy, `class-of`,
;;; `find-class`, `defclass`, instances and slots. Tier 1: compiled and SBCL
;;; only. Class precedence lists are tested through `typep` and `subtypep`:
;;; the MOP's reader is `sb-mop:class-precedence-list` in SBCL, and this file
;;; must stay portable.

;; The metaclasses.
(class-name (find-class 'standard-class))
(class-name (class-of (find-class 'standard-class)))
(eq (class-of (find-class 'standard-class)) (find-class 'standard-class))
(class-name (class-of (find-class 'integer)))
(class-name (class-of (find-class 'standard-object)))
(class-name (class-of (find-class 'structure-object)))

;; Every object has a class.
(class-name (class-of 5))
(class-name (class-of (expt 2 70)))
(class-name (class-of 1/2))
(class-name (class-of #\a))
(class-name (class-of 'a))
(class-name (class-of :a))
(class-name (class-of nil))
(class-name (class-of '(1)))
(class-name (class-of (make-hash-table)))
(class-name (class-of #'car))
(class-name (class-of *package*))
(class-name (class-of (make-array '(2 2))))
(subtypep (class-of "abc") 'string)
(subtypep (class-of #(1)) 'vector)
(subtypep (class-of #*1) 'bit-vector)
(subtypep (class-of 1.5) 'float)

;; find-class.
(find-class 'no-such-class nil)
(handler-case (find-class 'no-such-class) (error () :error))
(eq (find-class 'integer) (find-class 'integer))

;; Class objects as type specifiers, and the precedence of built-in classes.
(typep nil (find-class 'symbol))
(typep nil (find-class 'list))
(typep nil (find-class 'null))
(typep "a" (find-class 'vector))
(typep "a" (find-class 'sequence))
(typep 5 (find-class 'rational))
(typep 5 (find-class 'string))
(subtypep (find-class 'null) (find-class 'list))
(subtypep (find-class 'string) 'array)
(subtypep (find-class 'integer) (find-class 'float))

;; Structures and conditions have classes too.
(defstruct cpoint x)
(defstruct (cpoint3 (:include cpoint)) z)
(class-name (class-of (make-cpoint)))
(class-name (class-of (find-class 'cpoint)))
(subtypep (find-class 'cpoint3) (find-class 'cpoint))
(subtypep (find-class 'cpoint) 'structure-object)
(typep (make-cpoint3) (find-class 'cpoint))
(handler-case (error 'type-error :datum 1 :expected-type 'string) (error (c) (class-name (class-of c))))
(subtypep (find-class 'type-error) (find-class 'error))
(subtypep (find-class 'error) 'condition)

;; defclass and instances.
(class-name (defclass point () ((x :initarg :x :initform 0 :accessor point-x) (y :initarg :y :reader point-y :writer set-point-y))))
(defvar *p* (make-instance 'point :x 3 :y 4))
(list (point-x *p*) (point-y *p*))
(point-x (make-instance 'point))
(progn (setf (point-x *p*) 10) (point-x *p*))
(progn (set-point-y 20 *p*) (point-y *p*))
(slot-value *p* 'x)
(progn (setf (slot-value *p* 'x) 5) (slot-value *p* 'x))
(slot-boundp *p* 'y)
(slot-boundp (make-instance 'point) 'y)
(progn (slot-makunbound *p* 'y) (slot-boundp *p* 'y))
(list (slot-exists-p *p* 'x) (slot-exists-p *p* 'z))
(handler-case (slot-value *p* 'y) (unbound-slot () :unbound))
(handler-case (slot-value *p* 'z) (error () :missing))
(handler-case (make-instance 'point :z 1) (program-error () :invalid))
(point-x (make-instance 'point :z 1 :allow-other-keys t))
(handler-case (make-instance 'point :x) (program-error () :odd))
(eq (class-of *p*) (find-class 'point))
(typep *p* 'point)
(typep *p* 'standard-object)
(type-of *p*)

;; Inheritance: a slot defined again, initargs merged, the most specific initform.
(class-name (defclass point3 (point) ((x :initarg :px :initform 1) (z :initarg :z :initform 2 :accessor point-z))))
(let ((p (make-instance 'point3))) (list (point-x p) (point-z p)))
(let ((p (make-instance 'point3 :px 7 :z 8 :y 9))) (list (point-x p) (point-y p) (point-z p)))
(let ((p (make-instance 'point3 :x 6))) (point-x p))
(typep (make-instance 'point3) 'point)
(subtypep 'point3 'point)
(subtypep 'point 'point3)

;; Class slots are shared by every instance and by subclasses that do not define them again.
(class-name (defclass counter () ((count :allocation :class :initform 0 :accessor counter-count) (own :initform 1))))
(class-name (defclass sub-counter (counter) ()))
(let ((a (make-instance 'counter)) (b (make-instance 'counter)) (c (make-instance 'sub-counter))) (setf (counter-count a) 5) (list (counter-count b) (counter-count c)))

;; Default initargs, and a supplied initarg overriding one.
(class-name (defclass labelled () ((label :initarg :label :reader label)) (:default-initargs :label "none")))
(list (label (make-instance 'labelled)) (label (make-instance 'labelled :label "given")))
(class-name (defclass sub-labelled (labelled) () (:default-initargs :label "sub")))
(label (make-instance 'sub-labelled))

;; initialize-instance and reinitialize-instance.
(let ((p (make-instance 'point :x 1 :y 2))) (reinitialize-instance p :y 3) (list (point-x p) (point-y p)))
(let ((p (allocate-instance (find-class 'point)))) (list (slot-boundp p 'x) (progn (initialize-instance p :y 5) (list (point-x p) (point-y p)))))

;; A class whose superclasses cannot be ordered is an error, and nothing is left behind.
(class-name (defclass ca () ()))
(class-name (defclass cb (ca) ()))
(handler-case (defclass cc (ca cb) ()) (error () :inconsistent))
(find-class 'cc nil)

;; A superclass may be defined after its subclass.
(class-name (defclass early (late) ((e :initform 1 :reader early-e))))
(handler-case (make-instance 'early) (error () :not-yet))
(class-name (defclass late () ((l :initform 2 :reader late-l))))
(let ((x (make-instance 'early))) (list (early-e x) (late-l x)))

;; Redefinition keeps the class object.
(defvar *point-class* (find-class 'point))
(class-name (defclass point () ((x :initarg :x :initform 0 :accessor point-x) (y :initarg :y :reader point-y :writer set-point-y))))
(eq *point-class* (find-class 'point))
(point-x (make-instance 'point :x 11))

;; Classes are instances too.
(typep (find-class 'point) 'standard-class)
(typep (find-class 'point) 'class)

;; Classes in subtypep.
(subtypep 'point 'standard-object)
(subtypep 'point 'structure-object)
(subtypep (find-class 'point) 'point)
(typep (make-instance 'point) (find-class 'point))
(subtypep 'standard-class 'class)
(subtypep 'point '(or point integer))
(subtypep '(and point integer) nil)
(subtypep 'point 'function)
(subtypep 'standard-object 't)
(class-name (defclass tac-a () (x)))
(class-name (defclass tac-b () (y)))
(class-name (defclass tac-c () (z)))
(class-name (defclass tac-ab (tac-a tac-b) ()))
(class-name (defclass tac-ac (tac-a tac-c) ()))
(class-name (defclass tac-bc (tac-b tac-c) ()))
(class-name (defclass tac-abc (tac-ab tac-ac tac-bc) ()))
(multiple-value-list (subtypep 'tac-ab 'tac-a))
(multiple-value-list (subtypep 'tac-ab 'tac-c))
(multiple-value-list (subtypep 'tac-a 'tac-ab))
(multiple-value-list (subtypep 'tac-abc 'tac-bc))
(multiple-value-list (subtypep 'tac-ab 'tac-abc))
(typep (make-instance 'tac-abc) 'tac-c)
(typep (make-instance 'tac-ab) 'tac-c)

;; A structure is a class once its defstruct has run, not while it is being expanded.
(unless (find-class 'guarded-struct nil) (defstruct guarded-struct a))
(not (null (fboundp 'make-guarded-struct)))
(class-name (find-class 'guarded-struct))
;; A boolean combination of classes that no object can satisfy.
(multiple-value-list (subtypep '(and (or tac-a tac-b) (or (not tac-a) (not tac-b)) (or tac-a tac-c) (or (not tac-a) (not tac-c)) (or tac-b tac-c) (or (not tac-b) (not tac-c))) nil))
(multiple-value-list (subtypep t '(not (and (or tac-a tac-b) (or (not tac-a) (not tac-b)) (or tac-a tac-c) (or (not tac-a) (not tac-c)) (or tac-b tac-c) (or (not tac-b) (not tac-c))))))
