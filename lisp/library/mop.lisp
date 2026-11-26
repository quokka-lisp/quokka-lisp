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
;;;; does, and every reader stays as cheap as the `%` reader it calls.

;;; ------------------------------------------------------------------------
;;; Classes

(defun class-direct-superclasses (class) (%class-direct-superclasses class))
(defun class-direct-subclasses (class) (%class-direct-subclasses class))
(defun class-precedence-list (class) (%class-precedence-list class))
(defun class-direct-slots (class) (%class-direct-slots class))
(defun class-slots (class) (%class-slots class))
(defun class-direct-default-initargs (class) (%class-direct-default-initargs class))
(defun class-default-initargs (class) (%class-default-initargs class))
(defun class-finalized-p (class) (if (%class-finalized-p class) t nil))

;;; Finalizing computes a class's precedence list, slots and default
;;; initargs; Quokka does it as soon as every superclass is defined, so
;;; this only has work to do for a class that was waiting for one.
(defun finalize-inheritance (class)
  (unless (%class-finalized-p class)
    (%finalize-class class)
    (unless (%class-finalized-p class)
      (error "a class whose superclasses are not all defined cannot be finalized"
             (%class-name class))))
  class)

;;; An instance of CLASS that is never initialised, kept on the class's
;;; property list, so that a program can ask what methods would apply to
;;; one without making one of its own (AMOP).
(defun class-prototype (class)
  (finalize-inheritance class)
  (or (getf (%class-plist class) '%prototype)
      (let ((prototype (allocate-instance class)))
        (%set-class-plist class (list* '%prototype prototype (%class-plist class)))
        prototype)))

;;; ------------------------------------------------------------------------
;;; Slot definitions

(defun slot-definition-name (slot) (%slot-definition-name slot))
(defun slot-definition-initargs (slot) (%slot-definition-initargs slot))
(defun slot-definition-initform (slot) (%slot-definition-initform slot))
(defun slot-definition-initfunction (slot) (%slot-definition-initfunction slot))
(defun slot-definition-allocation (slot) (%slot-definition-allocation slot))
(defun slot-definition-type (slot) (%slot-definition-type slot))
(defun slot-definition-readers (slot) (%slot-definition-readers slot))
(defun slot-definition-writers (slot) (%slot-definition-writers slot))

;;; An instance slot's place in the slot vector; a shared slot's cell, the
;;; cons whose cdr holds its value (chapter 73).
(defun slot-definition-location (slot) (%slot-definition-location slot))

;;; The slot at LOCATION, a place `slot-definition-location` gave, read
;;; straight from the slot vector: no check that the instance is current,
;;; no `slot-unbound` (AMOP, for code that has done those itself).
(defun standard-instance-access (instance location)
  (aref (%instance-slots instance) location))

(defun funcallable-standard-instance-access (instance location)
  (aref (%instance-slots instance) location))

;;; ------------------------------------------------------------------------
;;; Generic functions and methods

(defun generic-function-name (gf) (%gf-name gf))
(defun generic-function-methods (gf) (%gf-methods gf))
(defun generic-function-lambda-list (gf) (%gf-lambda-list gf))
(defun generic-function-method-combination (gf) (%gf-method-combination gf))
(defun generic-function-method-class (gf) (%gf-method-class gf))
(defun generic-function-declarations (gf) (%gf-declarations gf))

;;; The required parameters in the order dispatch compares them: the
;;; `:argument-precedence-order` if one was given, else the lambda list's.
(defun generic-function-argument-precedence-order (gf)
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

(defun method-specializers (method) (%method-specializers method))
(defun method-lambda-list (method) (%method-lambda-list method))
(defun method-generic-function (method) (%method-generic-function method))

;;; A method's function is Quokka's own: a function of the argument list and
;;; the effective method of the methods after it (chapter 74). AMOP's takes
;;; a list of the next methods instead.
(defun method-function (method) (%method-function method))

;;; The direct slot definition a reader or writer method was made for.
(defun accessor-method-slot-definition (method)
  (let ((class (if (typep method 'standard-writer-method)
                   (second (%method-specializers method))
                   (first (%method-specializers method)))))
    (find (%method-slot-name method) (%class-direct-slots class) :key #'%slot-definition-name)))

(defun eql-specializer-object (specializer) (%eql-specializer-object specializer))

;;; The methods with SPECIALIZER, a class or an `eql` specializer, among
;;; their specializers, and their generic functions; `add-method` and
;;; `remove-method` keep them with each specializer (`generic-functions.lisp`).
(defun specializer-direct-methods (specializer)
  (copy-list (%specializer-methods specializer)))

(defun specializer-direct-generic-functions (specializer)
  (let ((functions nil))
    (dolist (method (%specializer-methods specializer) (nreverse functions))
      (pushnew (%method-generic-function method) functions))))

(defun find-method-combination (gf name options)
  gf
  (%method-combination-designated (cons name options)))

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
  (%set-funcallable-instance-function instance function))
