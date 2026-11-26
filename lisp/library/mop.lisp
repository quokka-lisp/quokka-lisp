;;;; mop.lisp — the introspective half of the metaobject protocol (phase 3,
;;;; CLOS sub-step 7, #124)
;;;;
;;;; Classes, slot definitions, generic functions and methods are objects
;;;; (chapters 73 and 74), and a program can ask them about themselves: a
;;;; class for its superclasses, its precedence list and its slots, a
;;;; generic function for its methods and lambda list, a method for its
;;;; specializers. *The Art of the Metaobject Protocol* (Kiczales, des
;;;; Rivières and Bobrow, 1991) names these readers, and every CLOS since
;;;; has them, outside the standard: SBCL in `SB-MOP`, Quokka in
;;;; `QUOKKA-MOP` (`packages.lisp` makes the package and puts these names
;;;; in it).
;;;;
;;;; This is the *introspective* half: a program can look, but not change
;;;; how the system works. The *intercessory* half, a program's own
;;;; metaclasses, `slot-value-using-class`, `compute-discriminating-function`,
;;;; is a non-goal (`docs/clos.md`). That is also why the readers here are
;;;; ordinary functions, where AMOP makes them generic: a method on one
;;;; could only change what the system reports about itself, never what it
;;;; does. Each checks the kind of object it is given and calls the `%`
;;;; reader Quokka's own code uses.

;;; Each reader first checks that it was given the kind of metaobject it
;;; reads. The `%` readers index a slot vector and would answer with some
;;; other slot of any instance, and an inspector passes these readers
;;; whatever it is shown; SBCL's are generic functions and signal
;;; `no-applicable-method`, and Quokka signals the same kind of error.
(defun %check-metaobject (object type reader)
  (unless (typep object type)
    (error 'error "no method of this generic function applies to these arguments"
           (list reader object))))

;;; ------------------------------------------------------------------------
;;; Classes

(defun class-direct-superclasses (class)
  (%check-metaobject class 'class 'class-direct-superclasses)
  (%class-direct-superclasses class))
(defun class-direct-subclasses (class)
  (%check-metaobject class 'class 'class-direct-subclasses)
  (%class-direct-subclasses class))
(defun class-precedence-list (class)
  (%check-metaobject class 'class 'class-precedence-list)
  (%class-precedence-list class))
(defun class-direct-default-initargs (class)
  (%check-metaobject class 'class 'class-direct-default-initargs)
  (%class-direct-default-initargs class))
(defun class-default-initargs (class)
  (%check-metaobject class 'class 'class-default-initargs)
  (%class-default-initargs class))
(defun class-finalized-p (class)
  (%check-metaobject class 'class 'class-finalized-p)
  (if (%class-finalized-p class) t nil))

;;; A structure class's slots are its layout's (chapter 61), which
;;; `defstruct` keeps, so its slot definitions are made from the layout when
;;; first asked for and kept on the class's property list until a
;;; redefinition replaces the layout's slots: the direct ones are those the
;;; structure adds to the one it includes.
(defun class-direct-slots (class)
  (%check-metaobject class 'class 'class-direct-slots)
  (if (%structure-class-p class)
      (first (%structure-slot-definitions class))
      (%class-direct-slots class)))

(defun class-slots (class)
  (%check-metaobject class 'class 'class-slots)
  (if (%structure-class-p class)
      (second (%structure-slot-definitions class))
      (%class-slots class)))

(defun %structure-class-p (class)
  (eq (class-of class) (find-class 'structure-class)))

(defun %structure-slot-definitions (class)
  (let* ((layout (%structure-class-layout class))
         (descriptions (and layout (%layout-slots layout)))
         (kept (getf (%class-plist class) '%structure-slots)))
    (if (and kept (eq (car kept) descriptions))
        (cdr kept)
        (let* ((included (and layout (%layout-include layout)
                              (mapcar #'car (%layout-slots (%layout-include layout)))))
               (made (list (mapcar (lambda (description) (%structure-slot-definition description nil))
                                   (remove-if (lambda (description) (member (car description) included))
                                              descriptions))
                           (mapcar (lambda (description) (%structure-slot-definition description t))
                                   descriptions))))
          (%set-class-plist class (list* '%structure-slots (cons descriptions made)
                                         (%remove-plist-key (%class-plist class) '%structure-slots)))
          made))))

;;; A slot definition for a structure's slot description, `(name index
;;; initform type read-only)`.
(defun %structure-slot-definition (description effective)
  (let ((initform (third description)))
    (%make-slot-definition (if effective
                               'standard-effective-slot-definition
                               'standard-direct-slot-definition)
                           :name (first description)
                           :initform initform
                           :initfunction (if initform (lambda () (eval initform)) nil)
                           :type (fourth description)
                           :location (if effective (second description) nil))))

;;; Finalizing computes a class's precedence list, slots and default
;;; initargs; Quokka does it as soon as every superclass is defined, so
;;; this only has work to do for a class that was waiting for one.
(defun finalize-inheritance (class)
  (%check-metaobject class 'class 'finalize-inheritance)
  (unless (%class-finalized-p class)
    (%finalize-class class)
    (unless (%class-finalized-p class)
      (error "a class whose superclasses are not all defined cannot be finalized"
             (%class-name class))))
  class)

;;; An instance of CLASS that is never initialised, kept on the class's
;;; property list, so that a program can ask what methods would apply to
;;; one without making one of its own (AMOP). A built-in class cannot
;;; allocate one, so its prototype is an object of the class, as in SBCL;
;;; nil is the prototype of `null`, so a kept answer is told from none by
;;; a marker.
(defun class-prototype (class)
  (%check-metaobject class 'class 'class-prototype)
  (finalize-inheritance class)
  (let ((kept (getf (%class-plist class) '%prototype '%none)))
    (if (eq kept '%none)
        (let ((prototype (if (eq (class-of class) (find-class 'built-in-class))
                             (%built-in-prototype (%class-name class))
                             (allocate-instance class))))
          (%set-class-plist class (list* '%prototype prototype (%class-plist class)))
          prototype)
        kept)))

;;; An object of the built-in class NAME. `t` has none in SBCL either, and
;;; neither do the classes of objects Quokka cannot make yet.
(defun %built-in-prototype (name)
  (case name
    ((number real rational integer fixnum) 0)
    (bignum (ash 1 100))
    (ratio (/ 1 2))
    ((float double-float) (float 0))
    (character #\*)
    ((symbol null list sequence) nil)
    (cons (list nil))
    ((array simple-array vector simple-vector) (vector))
    ((bit-vector simple-bit-vector) (make-array 0 :element-type 'bit))
    ((string simple-string) (make-string 0))
    (hash-table (make-hash-table))
    (function #'identity)
    (package (find-package "COMMON-LISP"))
    (readtable (copy-readtable nil))
    ((stream string-stream) (make-string-input-stream ""))
    (t (error "this class has no prototype" name))))

;;; ------------------------------------------------------------------------
;;; Slot definitions

(defun slot-definition-name (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-name)
  (%slot-definition-name slot))
(defun slot-definition-initargs (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-initargs)
  (%slot-definition-initargs slot))
(defun slot-definition-initform (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-initform)
  (%slot-definition-initform slot))
(defun slot-definition-initfunction (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-initfunction)
  (%slot-definition-initfunction slot))
(defun slot-definition-allocation (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-allocation)
  (%slot-definition-allocation slot))
(defun slot-definition-type (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-type)
  (%slot-definition-type slot))
(defun slot-definition-readers (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-readers)
  (%slot-definition-readers slot))
(defun slot-definition-writers (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-writers)
  (%slot-definition-writers slot))

;;; An instance slot's place in the slot vector; a shared slot's cell, the
;;; cons whose cdr holds its value (chapter 73).
(defun slot-definition-location (slot)
  (%check-metaobject slot 'slot-definition 'slot-definition-location)
  (%slot-definition-location slot))

;;; The slot at LOCATION, a place `slot-definition-location` gave, read
;;; straight from the slot vector: no check that the instance is current,
;;; no `slot-unbound` (AMOP, for code that has done those itself).
(defun standard-instance-access (instance location)
  (aref (%instance-slots instance) location))

(defun funcallable-standard-instance-access (instance location)
  (aref (%instance-slots instance) location))

;;; ------------------------------------------------------------------------
;;; Generic functions and methods

(defun generic-function-name (gf)
  (%check-metaobject gf 'generic-function 'generic-function-name)
  (%gf-name gf))
(defun generic-function-methods (gf)
  (%check-metaobject gf 'generic-function 'generic-function-methods)
  (%gf-methods gf))
(defun generic-function-lambda-list (gf)
  (%check-metaobject gf 'generic-function 'generic-function-lambda-list)
  (%gf-lambda-list gf))
(defun generic-function-method-combination (gf)
  (%check-metaobject gf 'generic-function 'generic-function-method-combination)
  (%gf-method-combination gf))
(defun generic-function-method-class (gf)
  (%check-metaobject gf 'generic-function 'generic-function-method-class)
  (%gf-method-class gf))
(defun generic-function-declarations (gf)
  (%check-metaobject gf 'generic-function 'generic-function-declarations)
  (%gf-declarations gf))

;;; The required parameters in the order dispatch compares them: the
;;; `:argument-precedence-order` if one was given, else the lambda list's.
(defun generic-function-argument-precedence-order (gf)
  (%check-metaobject gf 'generic-function 'generic-function-argument-precedence-order)
  (let ((required (%required-parameters (%gf-lambda-list gf)))
        (order (%gf-argument-precedence-order gf)))
    (if order
        (mapcar (lambda (position) (nth position required)) order)
        required)))

(defun %required-parameters (lambda-list)
  (let ((found nil))
    (dolist (item lambda-list (nreverse found))
      (if (member item %lambda-markers) (return (nreverse found)))
      (push (if (consp item) (car item) item) found))))

(defun method-specializers (method)
  (%check-metaobject method 'method 'method-specializers)
  (%method-specializers method))
(defun method-lambda-list (method)
  (%check-metaobject method 'method 'method-lambda-list)
  (%method-lambda-list method))
(defun method-generic-function (method)
  (%check-metaobject method 'method 'method-generic-function)
  (%method-generic-function method))

;;; A method's function is Quokka's own: a function of the argument list and
;;; the effective method of the methods after it (chapter 74). AMOP's takes
;;; a list of the next methods instead.
(defun method-function (method)
  (%check-metaobject method 'method 'method-function)
  (%method-function method))

;;; The direct slot definition a reader or writer method was made for.
(defun accessor-method-slot-definition (method)
  (%check-metaobject method 'standard-accessor-method 'accessor-method-slot-definition)
  (let ((class (if (typep method 'standard-writer-method)
                   (second (%method-specializers method))
                   (first (%method-specializers method)))))
    (find (%method-slot-name method) (%class-direct-slots class) :key #'%slot-definition-name)))

(defun eql-specializer-object (specializer)
  (%check-metaobject specializer 'eql-specializer 'eql-specializer-object)
  (%eql-specializer-object specializer))

;;; The methods with SPECIALIZER, a class or an `eql` specializer, among
;;; their specializers, and their generic functions; `add-method` and
;;; `remove-method` keep them with each specializer (`generic-functions.lisp`).
(defun specializer-direct-methods (specializer)
  (%check-metaobject specializer 'specializer 'specializer-direct-methods)
  (copy-list (%specializer-methods specializer)))

(defun specializer-direct-generic-functions (specializer)
  (%check-metaobject specializer 'specializer 'specializer-direct-generic-functions)
  (let ((functions nil))
    (dolist (method (%specializer-methods specializer) (nreverse functions))
      (pushnew (%method-generic-function method) functions))))

;;; The method combination NAME with OPTIONS, or nil if there is no
;;; combination of that name, as in SBCL.
(defun find-method-combination (gf name options)
  (%check-metaobject gf 'generic-function 'find-method-combination)
  (if (gethash name %combinations)
      (%method-combination-designated (cons name options))
      nil))

;;; A `defmethod` lambda list without its specializers, and its
;;; specializers alone, `t` for an unspecialized parameter (AMOP).
(defun extract-lambda-list (specialized-lambda-list)
  (%plain-lambda-list specialized-lambda-list))

(defun extract-specializer-names (specialized-lambda-list)
  (let ((names nil))
    (dolist (item specialized-lambda-list (nreverse names))
      (if (member item %lambda-markers) (return (nreverse names)))
      (push (if (consp item) (second item) t) names))))

;;; ------------------------------------------------------------------------
;;; Funcallable instances

;;; What calling INSTANCE calls (AMOP); the function a generic function is
;;; given this way is its discriminating function.
(defun set-funcallable-instance-function (instance function)
  (%check-metaobject instance 'funcallable-standard-object 'set-funcallable-instance-function)
  (%set-funcallable-instance-function instance function))
