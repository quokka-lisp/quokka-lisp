;;;; clos.lisp — classes and instances (phase 3, step A2, #124)
;;;;
;;;; Every object has a class, and a program can define its own:
;;;;
;;;;   (defclass point () ((x :initarg :x :accessor point-x)))
;;;;   (point-x (make-instance 'point :x 3))      ; => 3
;;;;
;;;; This is the object system of Common Lisp, CLOS, built the way Part I of
;;;; *The Art of the Metaobject Protocol* (Kiczales, des Rivières and Bobrow,
;;;; 1991) builds its "Closette": classes are themselves instances, of
;;;; classes called *metaclasses*, and `standard-class` is an instance of
;;;; itself. `docs/clos.md` has the design and the order of the work.
;;;;
;;;; An instance is a heap object of its own type (`crate::instance`): a
;;;; *wrapper*, which belongs to its class and names its slots in order, and
;;;; a *slot vector*. Everything about classes is here; Rust only allocates
;;;; instances and calls the funcallable ones.
;;;;
;;;; This file has the workers: `%make-instance`, `%shared-initialize` and
;;;; their kin, and the slot functions, which stay ordinary functions.
;;;; `generic-functions.lisp` makes `make-instance`, `initialize-instance`
;;;; and `shared-initialize` generic, with the workers as their standard
;;;; methods; that is Closette's order too. This file must not define
;;;; those public names, or a warm rebuild would replace the generic
;;;; functions.
;;;;
;;;; The names that belong to the metaobject protocol and not to Common Lisp
;;;; (`metaobject`, `specializer`, the slot definition classes, ...) are in
;;;; package `QUOKKA-MOP` (`packages.lisp`, sub-step 7), as SBCL's are in
;;;; `SB-MOP`; `condition-class`, which SBCL keeps internal, stays in
;;;; `QUOKKA`.

;;; ------------------------------------------------------------------------
;;; Wrappers
;;;
;;; A wrapper is a simple vector #(CLASS SLOT-NAMES STATE): the class whose
;;; instances it describes, a simple vector of the names of their slots in
;;; the order the slot vector keeps them, and a state. The state is nil
;;; while the class uses the wrapper. When the class is redefined with other
;;; slots, or `make-instances-obsolete` is called, the class gets a new
;;; wrapper and the old one becomes *obsolete*: its state is
;;; `(:obsolete . cells)`, CELLS the old class's shared slots as `(name .
;;; cell)`, which the update of an instance still laid out by it needs
;;; (CLHS 4.3.6.1: a slot shared before and local now keeps its value).

(defun %make-wrapper (class slot-names) (vector class slot-names nil))
(defun %wrapper-class (wrapper) (aref wrapper 0))
(defun %wrapper-slot-names (wrapper) (aref wrapper 1))
(defun %wrapper-state (wrapper) (aref wrapper 2))

;;; The shared slots of CLASS as it is now, as `(name . cell)`.
(defun %class-shared-cells (class)
  (let ((cells nil))
    (dolist (slot (%class-slots class) (nreverse cells))
      (let ((location (%slot-definition-location slot)))
        (when (consp location) (push (cons (%slot-definition-name slot) location) cells))))))

(defun %obsolete-wrapper (wrapper class)
  (setf (aref wrapper 2) (cons :obsolete (%class-shared-cells class))))

;;; What an unbound slot holds: an object nothing else can be. A `defvar`,
;;; so that a warm rebuild keeps the same one and instances made before it
;;; keep their unbound slots unbound.
(defvar %slot-unbound-marker (make-symbol "UNBOUND-SLOT"))

;;; Made anew whenever something every generic function's dispatch may
;;; depend on changes: a class is defined or redefined, a method
;;; combination defined, a method added to one of the few functions other
;;; caches are worked out from (`%epoch-functions`). Each generic function
;;; empties its cache when its own copy of this differs
;;; (`generic-functions.lisp`).
;;; Each epoch is a fresh cons, compared with `eq`, rather than a number:
;;; a counter would keep counting across warm rebuilds, so two generations
;;; of the world would hold different numbers and differ word for word,
;;; and resetting it could make a stale cache look current.
(defvar %dispatch-epoch (list :epoch))

(defun %invalidate-dispatch () (setq %dispatch-epoch (list :epoch)))

;;; ------------------------------------------------------------------------
;;; Class metaobjects
;;;
;;; A class is a standard instance whose slots are, in this order, those of
;;; the class `class`, which every metaclass inherits and none adds to. So
;;; their places are fixed, and these readers can index the slot vector
;;; directly; they are what the bootstrap needs before `slot-value` exists.

(defvar %class-slot-names
  '(name direct-superclasses direct-subclasses precedence-list direct-slots slots
    direct-default-initargs default-initargs finalized-p wrapper documentation plist))

(defun %class-ref (class index) (aref (%instance-slots class) index))
(defun %class-set (class index value) (setf (aref (%instance-slots class) index) value))

(defun %class-name (class) (%class-ref class 0))
(defun %class-direct-superclasses (class) (%class-ref class 1))
(defun %class-direct-subclasses (class) (%class-ref class 2))
(defun %class-precedence-list (class) (%class-ref class 3))
(defun %class-direct-slots (class) (%class-ref class 4))
(defun %class-slots (class) (%class-ref class 5))
(defun %class-direct-default-initargs (class) (%class-ref class 6))
(defun %class-default-initargs (class) (%class-ref class 7))
(defun %class-finalized-p (class) (%class-ref class 8))
(defun %class-wrapper (class) (%class-ref class 9))
(defun %class-documentation (class) (%class-ref class 10))
(defun %class-plist (class) (%class-ref class 11))

(defun %set-class-name (class value) (%class-set class 0 value))
(defun %set-class-direct-superclasses (class value) (%class-set class 1 value))
(defun %set-class-direct-subclasses (class value) (%class-set class 2 value))
(defun %set-class-precedence-list (class value) (%class-set class 3 value))
(defun %set-class-direct-slots (class value) (%class-set class 4 value))
(defun %set-class-slots (class value) (%class-set class 5 value))
(defun %set-class-direct-default-initargs (class value) (%class-set class 6 value))
(defun %set-class-default-initargs (class value) (%class-set class 7 value))
(defun %set-class-finalized-p (class value) (%class-set class 8 value))
(defun %set-class-wrapper (class value) (%class-set class 9 value))
(defun %set-class-documentation (class value) (%class-set class 10 value))
(defun %set-class-plist (class value) (%class-set class 11 value))

;;; A slot definition is a standard instance of
;;; `standard-direct-slot-definition` or `standard-effective-slot-definition`,
;;; with the slots of `standard-slot-definition` in this order.

(defvar %slot-definition-slot-names
  '(name initargs initform initfunction allocation type readers writers documentation
    location))

(defun %slot-definition-name (slot) (aref (%instance-slots slot) 0))
(defun %slot-definition-initargs (slot) (aref (%instance-slots slot) 1))
(defun %slot-definition-initform (slot) (aref (%instance-slots slot) 2))
(defun %slot-definition-initfunction (slot) (aref (%instance-slots slot) 3))
(defun %slot-definition-allocation (slot) (aref (%instance-slots slot) 4))
(defun %slot-definition-type (slot) (aref (%instance-slots slot) 5))
(defun %slot-definition-readers (slot) (aref (%instance-slots slot) 6))
(defun %slot-definition-writers (slot) (aref (%instance-slots slot) 7))
(defun %slot-definition-documentation (slot) (aref (%instance-slots slot) 8))
(defun %slot-definition-location (slot) (aref (%instance-slots slot) 9))
(defun %set-slot-definition-location (slot value) (setf (aref (%instance-slots slot) 9) value))

;;; ------------------------------------------------------------------------
;;; The table of classes

;;; Name to class. A `defvar`, so that a warm rebuild finds the classes it
;;; is rebuilding and refills them instead of making new ones.
(defvar %class-table (make-hash-table :test 'eq))

;;; The class named NAME. A structure's class is made the first time it is
;;; asked for (see `%structure-class`).
(defun find-class (symbol &optional (errorp t) environment)
  environment
  (let ((class (or (gethash symbol %class-table)
                   (and (%structure-type-layout symbol)
                        (get symbol '%structure-defined)
                        (%structure-class (%structure-type-layout symbol))))))
    (cond (class class)
          (errorp (error "there is no class with this name" symbol))
          (t nil))))

(defun %set-find-class (symbol class)
  ;; Only a class, or nil to take the name away: 5 was stored, and
  ;; `find-class` then answered 5 (#190).
  (unless (or (null class) (and (%instance-wrapper class) (typep class 'class)))
    (error 'type-error :datum class :expected-type '(or class null)))
  (if class
      (setf (gethash symbol %class-table) class)
      (remhash symbol %class-table))
  class)

(defsetf find-class (symbol &optional errorp environment) (class)
  `(progn ,errorp ,environment (%set-find-class ,symbol ,class)))



;;; ------------------------------------------------------------------------
;;; The class precedence list (CLHS 4.3.5)
;;;
;;; Each class orders itself before its direct superclasses, and those in
;;; the order it lists them. The precedence list is a total order that keeps
;;; all of those orders: a topological sort. When several classes could come
;;; next, CLHS chooses the one that is a direct superclass of the class
;;; furthest to the right in the list so far, which keeps a class's
;;; superclasses close to it. Python and Dylan use a different rule, C3,
;;; which refuses some hierarchies CLOS accepts.

;;; CLASS and all its superclasses, each once.
(defun %all-superclasses (class)
  (let ((seen nil))
    (labels ((walk (c)
               (unless (member c seen)
                 (push c seen)
                 (dolist (super (%class-direct-superclasses c)) (walk super)))))
      (walk class))
    (nreverse seen)))

;;; The pairs (A . B), "A comes before B", that CLASS's own definition asks for.
(defun %local-precedence (class)
  (let ((order (cons class (%class-direct-superclasses class)))
        (pairs nil))
    (do ((rest order (cdr rest)))
        ((null (cdr rest)) pairs)
      (push (cons (car rest) (car (cdr rest))) pairs))))

(defun %compute-class-precedence-list (class)
  (let* ((classes (%all-superclasses class))
         (pairs (let ((all nil))
                  (dolist (c classes all) (setq all (append (%local-precedence c) all)))))
         (result nil))
    (do () ((null classes) (nreverse result))
      (let ((candidates (remove-if (lambda (c) (some (lambda (pair) (eq (cdr pair) c)) pairs))
                                   classes)))
        (when (null candidates)
          (error "the superclasses of this class cannot be put in one order"
                 (%class-name class)))
        (let ((next (if (null (cdr candidates))
                        (car candidates)
                        (%precedence-tie candidates result))))
          (push next result)
          (setq classes (remove next classes))
          (setq pairs (remove-if (lambda (pair) (eq (car pair) next)) pairs)))))))

;;; Of CANDIDATES, the one that is a direct superclass of the class furthest
;;; to the right in RESULT (which is kept in reverse, rightmost first).
(defun %precedence-tie (candidates result)
  (dolist (c result (car candidates))
    (let ((found (find-if (lambda (candidate) (member candidate (%class-direct-superclasses c)))
                          candidates)))
      (when found (return found)))))

;;; ------------------------------------------------------------------------
;;; Slot definitions and the effective slots of a class (CLHS 7.5.3)

;;; A class made from nothing: a standard instance with a slot vector the
;;; size of a class's, and a wrapper set later.
(defun %allocate-class-object (wrapper)
  (%allocate-standard-instance wrapper
                               (make-array (length %class-slot-names) :initial-element nil)))

(defun %make-slot-definition (class-name &key name initargs initform initfunction
                                              (allocation :instance) (type t) readers
                                              writers documentation location)
  (let ((slot (%allocate-standard-instance
               (%class-wrapper (find-class class-name))
               (vector name initargs initform initfunction allocation type readers writers
                       documentation location))))
    slot))

(defun %make-direct-slot-definition (spec)
  (apply #'%make-slot-definition 'standard-direct-slot-definition spec))

;;; The effective slot for NAME, made from DIRECT, the direct slots of that
;;; name along the precedence list, most specific first: the initform and
;;; allocation of the first that has one, every initarg, and the
;;; conjunction of the types.
(defun %effective-slot (name direct)
  (let ((initargs nil) (types nil) (with-initform nil))
    (dolist (slot direct)
      (dolist (initarg (%slot-definition-initargs slot)) (pushnew initarg initargs))
      (unless (eq (%slot-definition-type slot) t) (pushnew (%slot-definition-type slot) types))
      (when (and (null with-initform) (%slot-definition-initfunction slot))
        (setq with-initform slot)))
    (%make-slot-definition 'standard-effective-slot-definition
                           :name name
                           :initargs (nreverse initargs)
                           :initform (and with-initform (%slot-definition-initform with-initform))
                           :initfunction (and with-initform
                                              (%slot-definition-initfunction with-initform))
                           :allocation (%slot-definition-allocation (car direct))
                           :type (cond ((null types) t)
                                       ((null (cdr types)) (car types))
                                       (t (cons 'and types))))))

;;; The effective slots of a class whose precedence list is CPL, superclass
;;; slots first, as SBCL orders them. An instance slot's location is its
;;; place in the slot vector; a class slot's is a cons (name . value) kept
;;; by the class that defines it, shared by every subclass that does not
;;; define the slot again.
(defun %compute-slots (class cpl)
  (let ((names nil))
    (dolist (c (reverse cpl))
      (dolist (slot (%class-direct-slots c))
        (unless (member (%slot-definition-name slot) names)
          (setq names (append names (list (%slot-definition-name slot)))))))
    (let ((index 0) (slots nil))
      (dolist (name names (nreverse slots))
        (let* ((direct (let ((found nil))
                         (dolist (c cpl (nreverse found))
                           (dolist (slot (%class-direct-slots c))
                             (when (eq (%slot-definition-name slot) name) (push slot found))))))
               (effective (%effective-slot name direct)))
          (if (eq (%slot-definition-allocation effective) :class)
              (%set-slot-definition-location effective (%shared-slot-cell class cpl name))
              (progn (%set-slot-definition-location effective index)
                     (setq index (+ index 1))))
          (push effective slots))))))

;;; The cons holding the shared slot NAME: kept on the plist of the most
;;; specific class in CPL that defines NAME, and made (unbound) the first
;;; time, so that redefining a class keeps the value.
(defun %shared-slot-cell (class cpl name)
  class
  (let ((owner (find-if (lambda (c) (find name (%class-direct-slots c) :key #'%slot-definition-name))
                        cpl)))
    (let* ((cells (getf (%class-plist owner) '%shared-slots))
           (cell (assoc name cells)))
      (unless cell
        (setq cell (cons name %slot-unbound-marker))
        (%set-class-plist owner (list* '%shared-slots (cons cell cells)
                                       (%remove-plist-key (%class-plist owner) '%shared-slots))))
      cell)))

;;; Forget the shared slots CLASS no longer defines, so that one removed
;;; and later defined again starts from its initform, as in SBCL. An
;;; instance still laid out by an obsolete wrapper loses nothing: the
;;; wrapper keeps its own list of the old cells.
(defun %prune-shared-cells (class)
  (let ((cells (getf (%class-plist class) '%shared-slots)))
    (when cells
      (%set-class-plist
       class
       (list* '%shared-slots
              (remove-if-not (lambda (cell)
                               (find-if (lambda (slot)
                                          (and (eq (%slot-definition-name slot) (car cell))
                                               (eq (%slot-definition-allocation slot) :class)))
                                        (%class-direct-slots class)))
                             cells)
              (%remove-plist-key (%class-plist class) '%shared-slots))))))

(defun %remove-plist-key (plist key)
  (cond ((null plist) nil)
        ((eq (car plist) key) (%remove-plist-key (cddr plist) key))
        (t (list* (car plist) (cadr plist) (%remove-plist-key (cddr plist) key)))))

;;; The default initargs along the precedence list, most specific first,
;;; each key once: entries (key initform initfunction).
(defun %compute-default-initargs (cpl)
  (let ((result nil))
    (dolist (c cpl (nreverse result))
      (dolist (entry (%class-direct-default-initargs c))
        (unless (assoc (car entry) result) (push entry result))))))

;;; ------------------------------------------------------------------------
;;; Finalizing a class

;;; Is CLASS a placeholder for a superclass not yet defined?
(defun %forward-referenced-p (class)
  (eq (class-of class) (gethash 'forward-referenced-class %class-table)))

;;; Compute CLASS's precedence list, slots, default initargs and wrapper,
;;; unless a superclass is not yet defined; return whether it was done. A
;;; class whose instance slots are the same as before keeps its wrapper, so
;;; instances made before stay instances of it (a warm rebuild redefines
;;; every class while its instances are alive).
(defun %finalize-class (class)
  (if (some #'%forward-referenced-p (%all-superclasses class))
      (progn (%set-class-finalized-p class nil) nil)
      (let* ((cpl (%compute-class-precedence-list class))
             (slots (progn (%set-class-precedence-list class cpl) (%compute-slots class cpl)))
             (names (let ((found nil))
                      (dolist (slot slots (coerce (nreverse found) 'simple-vector))
                        (when (integerp (%slot-definition-location slot))
                          (push (%slot-definition-name slot) found)))))
             (old (%class-wrapper class)))
        ;; A wrapper whose slots differ from the new ones is replaced, and
        ;; the instances laid out by it are updated when next touched
        ;; (`%ensure-current-instance`). Unchanged slots keep the wrapper:
        ;; nothing goes obsolete, as in SBCL, and a warm rebuild leaves
        ;; every library instance as it was.
        (unless (and old (equalp (%wrapper-slot-names old) names))
          (when old (%obsolete-wrapper old class))
          (%set-class-wrapper class (%make-wrapper class names)))
        (%set-class-slots class slots)
        (%set-class-default-initargs class (%compute-default-initargs cpl))
        (%initialize-shared-slots slots)
        (%set-class-finalized-p class t)
        t)))

;;; A shared slot gets its initform's value when its class is defined, as
;;; SBCL does, rather than when the first instance is made; one that
;;; already has a value keeps it when the class is redefined.
(defun %initialize-shared-slots (slots)
  (dolist (slot slots)
    (let ((location (%slot-definition-location slot)))
      (when (and (consp location)
                 (eq (cdr location) %slot-unbound-marker)
                 (%slot-definition-initfunction slot))
        (setf (cdr location) (funcall (%slot-definition-initfunction slot)))))))

;;; What the standard methods of `make-instances-obsolete` do: CLASS's
;;; instances are updated when next touched, with no slots added or
;;; discarded.
(defun %make-instances-obsolete (class)
  (let ((old (%class-wrapper class)))
    (when (and old (%class-finalized-p class))
      (%obsolete-wrapper old class)
      (%set-class-wrapper class (%make-wrapper class (%wrapper-slot-names old)))))
  class)

;;; Finalize CLASS, and every subclass that was finalized, again.
(defun %refinalize (class)
  (%finalize-class class)
  (dolist (sub (%class-direct-subclasses class))
    (%refinalize sub)))

(defun %ensure-finalized (class)
  (unless (or (%class-finalized-p class) (%finalize-class class))
    (error "a superclass of this class is not defined yet" (%class-name class)))
  class)

;;; Make CLASS's direct superclasses SUPERS, keeping every superclass's list
;;; of direct subclasses right.
(defun %set-superclasses (class supers)
  (dolist (old (%class-direct-superclasses class))
    (%set-class-direct-subclasses old (remove class (%class-direct-subclasses old))))
  (%set-class-direct-superclasses class supers)
  (dolist (super supers)
    (unless (member class (%class-direct-subclasses super))
      (%set-class-direct-subclasses super (append (%class-direct-subclasses super)
                                                  (list class))))))

;;; ------------------------------------------------------------------------
;;; class-of

;;; A structure's class is found from its layout (`%structure-class`), so
;;; it stays the structure's class when it is renamed. Packages, readtables
;;; and streams are structures too, but their classes are built-in ones.
(defun class-of (object)
  (let ((wrapper (%instance-wrapper object)))
    (cond (wrapper (%wrapper-class wrapper))
          ((and (%structure-layout object) (not (%standard-structure-name object)))
           (%structure-class (%structure-layout object)))
          (t (find-class (%built-in-class-name object))))))

;;; The name of the class of an object that is neither an instance nor a
;;; structure of a `defstruct` (`class-of`).
(defun %built-in-class-name (object)
  (cond ((null object) 'null)
        ((symbolp object) 'symbol)
        ((consp object) 'cons)
        ((integerp object) (if (%fixnump object) 'fixnum 'bignum))
        ((%ratiop object) 'ratio)
        ((floatp object) 'double-float)
        ((characterp object) 'character)
        ((simple-vector-p object) 'simple-vector)
        ((simple-string-p object) 'simple-string)
        ((simple-bit-vector-p object) 'simple-bit-vector)
        ((stringp object) 'string)
        ((bit-vector-p object) 'bit-vector)
        ((%simple-array-p object) 'simple-array)
        ((vectorp object) 'vector)
        ((arrayp object) 'array)
        ((functionp object) 'function)
        ((hash-table-p object) 'hash-table)
        ((%conditionp object)
         (if (gethash (%condition-type object) %class-table) (%condition-type object) 'condition))
        ((%standard-structure-name object))
        (t t)))

;;; ------------------------------------------------------------------------
;;; The bootstrap
;;;
;;; `standard-class` is an instance of itself, and a class's slots are
;;; described by slot definitions, which are instances of classes. Nothing
;;; can be made with `defclass` until those classes exist, so the first ones
;;; are made by hand, in stages, each of which can run again: a warm rebuild
;;; reloads this file while every class is in use, and each stage then
;;; refills the classes it finds instead of making new ones.

;;; The classes made by hand: (name metaclass direct-superclasses direct-slots).
(defvar %bootstrap-classes
  '((t built-in-class ())
    (standard-object standard-class (t))
    (function built-in-class (t))
    (metaobject standard-class (standard-object))
    (specializer standard-class (metaobject))
    (class standard-class (specializer) :class-slots)
    (built-in-class standard-class (class))
    (standard-class standard-class (class))
    (structure-class standard-class (class))
    (condition-class standard-class (class))
    (funcallable-standard-class standard-class (class))
    (forward-referenced-class standard-class (class))
    (funcallable-standard-object funcallable-standard-class (function standard-object))
    (slot-definition standard-class (metaobject))
    (standard-slot-definition standard-class (slot-definition) :slot-definition-slots)
    (direct-slot-definition standard-class (slot-definition))
    (effective-slot-definition standard-class (slot-definition))
    (standard-direct-slot-definition standard-class
     (standard-slot-definition direct-slot-definition))
    (standard-effective-slot-definition standard-class
     (standard-slot-definition effective-slot-definition))
    (structure-object structure-class (t))))

(defvar %metaclass-names
  '(built-in-class standard-class structure-class condition-class funcallable-standard-class
    forward-referenced-class))

(defun %bootstrap ()
  ;; 1. A class object for every name, reused if it exists.
  (dolist (entry %bootstrap-classes)
    (unless (gethash (car entry) %class-table)
      (let ((class (%allocate-class-object nil)))
        (%set-class-name class (car entry))
        (setf (gethash (car entry) %class-table) class))))
  ;; 2. Wrappers for the metaclasses' instances and the slot definitions',
  ;; whose slots are known without computing anything; then every class
  ;; made by hand gets its metaclass's wrapper.
  (flet ((give-wrapper (name slot-names)
           (let ((class (gethash name %class-table))
                 (names (coerce slot-names 'simple-vector)))
             (unless (and (%class-wrapper class)
                          (equalp (%wrapper-slot-names (%class-wrapper class)) names))
               (%set-class-wrapper class (%make-wrapper class names))))))
    (dolist (name %metaclass-names) (give-wrapper name %class-slot-names))
    (give-wrapper 'standard-direct-slot-definition %slot-definition-slot-names)
    (give-wrapper 'standard-effective-slot-definition %slot-definition-slot-names))
  (dolist (entry %bootstrap-classes)
    (%set-instance-wrapper (gethash (car entry) %class-table)
                           (%class-wrapper (gethash (second entry) %class-table))))
  ;; 3. Superclasses and direct slots, now that slot definitions can be made.
  (dolist (entry %bootstrap-classes)
    (let ((class (gethash (car entry) %class-table)))
      (%set-superclasses class (mapcar (lambda (name) (gethash name %class-table)) (third entry)))
      (%set-class-direct-slots
       class
       (mapcar (lambda (name) (%make-direct-slot-definition (list :name name)))
               (case (fourth entry)
                 (:class-slots %class-slot-names)
                 (:slot-definition-slots %slot-definition-slot-names)
                 (t nil))))))
  ;; 4. Precedence lists and effective slots.
  (dolist (entry %bootstrap-classes)
    (%finalize-class (gethash (car entry) %class-table))))

(%bootstrap)

;;; ------------------------------------------------------------------------
;;; Built-in classes
;;;
;;; The classes of CLHS figure 4-8, with SBCL's `fixnum`, `bignum`,
;;; `double-float` and the simple array classes, which `class-of` returns
;;; for those objects as SBCL's does. Each is (name direct-superclasses).

(defvar %built-in-classes
  '((number (t)) (real (number)) (rational (real)) (integer (rational))
    (fixnum (integer)) (bignum (integer)) (ratio (rational)) (float (real))
    (double-float (float)) (complex (number)) (character (t)) (symbol (t))
    (sequence (t)) (list (sequence)) (cons (list)) (null (symbol list))
    (array (t)) (simple-array (array)) (vector (array sequence))
    (simple-vector (vector simple-array)) (bit-vector (vector))
    (simple-bit-vector (bit-vector simple-array)) (string (vector))
    (simple-string (string simple-array)) (hash-table (t)) (package (t))
    (readtable (t)) (stream (t)) (string-stream (stream)) (random-state (t))
    (pathname (t)) (logical-pathname (pathname))))

(defun %ensure-plain-class (name metaclass-name supers)
  (let ((class (or (gethash name %class-table)
                   (let ((new (%allocate-class-object
                               (%class-wrapper (gethash metaclass-name %class-table)))))
                     (%set-class-name new name)
                     (setf (gethash name %class-table) new)))))
    (%set-instance-wrapper class (%class-wrapper (gethash metaclass-name %class-table)))
    (%set-superclasses class supers)
    (%finalize-class class)
    class))

(dolist (entry %built-in-classes)
  (%ensure-plain-class (car entry) 'built-in-class
                       (mapcar #'find-class (second entry))))

;;; ------------------------------------------------------------------------
;;; Condition classes
;;;
;;; The standard condition types become classes now, of metaclass
;;; `condition-class`, as in SBCL, where a condition is not a standard
;;; object. The lattice is the one the prelude keeps in
;;; `%condition-supertypes`. A condition is an instance of one of these
;;; classes (`conditions.lisp`).

(%ensure-plain-class 'condition 'condition-class (list (find-class t)))

(defun %ensure-condition-class (name)
  (or (let ((class (gethash name %class-table)))
        (and class (%class-finalized-p class) class))
      (let ((supers (cdr (%condition-entry name %condition-supertypes))))
        (%ensure-plain-class name 'condition-class
                             (mapcar #'%ensure-condition-class (or supers '(condition)))))))

(dolist (entry %condition-supertypes) (%ensure-condition-class (car entry)))

;;; ------------------------------------------------------------------------
;;; Structure classes

;;; The class of the structures LAYOUT describes, made the first time it is
;;; asked for, with the class of the structure it includes, or
;;; `structure-object`, as its superclass.
;;;
;;; The two find each other directly, not through the class's name: the
;;; class is kept under the structure name's property `%structure-class`,
;;; and the layout on the class's property list. A program may rename the
;;; class with `(setf class-name)`, and then `defclass` the old name, which
;;; puts a new class in the class table under it (CLHS 4.3.1, and SBCL);
;;; the structures keep the class they had, under its new name.
(defun %structure-class (layout)
  (let ((name (%layout-name layout)))
    (or (get name '%structure-class)
        (let ((class (gethash name %class-table)))
          (unless (and class (eq (class-of class) (gethash 'structure-class %class-table)))
            (setq class (%ensure-plain-class name 'structure-class
                                             (list (if (%layout-include layout)
                                                       (%structure-class (%layout-include layout))
                                                       (find-class 'structure-object))))))
          (%set-class-plist class (list* '%layout layout
                                         (%remove-plist-key (%class-plist class) '%layout)))
          (setf (get name '%structure-class) class)))))

;;; The layout of the structures whose class is CLASS, a structure class.
(defun %structure-class-layout (class)
  (getf (%class-plist class) '%layout))

;;; A structure of CLASS's layout with every slot's initform, evaluated in
;;; the order of the slots: what `make-instance` of a structure class makes
;;; (`generic-functions.lisp`). An initform is evaluated in the null lexical
;;; environment, which is the one a `defstruct` at top level has.
(defun %make-default-structure (class)
  (let* ((layout (%structure-class-layout class))
         (structure (%make-structure layout (%layout-length layout))))
    (dolist (description (%layout-slots layout) structure)
      (%set-structure-ref structure (second description) (eval (third description))))))

;;; ------------------------------------------------------------------------
;;; Defining classes

;;; The class SUPER names, or a placeholder for it when it is not defined
;;; yet (CLHS allows a superclass to be defined after its subclass). A
;;; placeholder is a class of metaclass `forward-referenced-class`; when
;;; `defclass` later defines that name, the placeholder becomes the class,
;;; so everything that refers to it sees the definition.
(defun %superclass-named (name)
  (if (symbolp name)
      (or (find-class name nil)
          (let ((placeholder (%allocate-class-object
                              (%class-wrapper (find-class 'forward-referenced-class)))))
            (%set-class-name placeholder name)
            (setf (gethash name %class-table) placeholder)
            placeholder))
      name))

;;; `(ensure-class name &key ...)`: define or redefine the class NAME and
;;; return it. A class that exists keeps its identity: its slots are
;;; refilled, and it and its subclasses are finalized again. A new class
;;; that cannot be finalized because its superclasses cannot be ordered is
;;; removed again before the error goes on.
(defun ensure-class (name &key direct-superclasses direct-slots direct-default-initargs
                               (metaclass 'standard-class) documentation &allow-other-keys)
  ;; Everything that can be refused is refused before anything is changed
  ;; (#189): a `defclass` that failed half way used to leave a class, or the
  ;; placeholder standing for one, with a wrapper and no superclasses.
  (unless (and name (symbolp name))
    (error 'type-error "a class is named by a symbol other than nil" name))
  (when (member name direct-superclasses)
    (error "a class cannot be its own superclass" name))
  (let* ((metaclass (%usable-metaclass metaclass))
         (existing (let ((found (find-class name nil)))
                     ;; A class is redefined only while NAME is its proper
                     ;; name (CLHS 4.3.1): one whose `class-name` has been
                     ;; changed is left as it is, and a new class made.
                     (when (and found (not (eq (%class-name found) name)))
                       (setq found nil))
                     ;; A placeholder that standard classes name as their
                     ;; superclass does not become a condition class: the
                     ;; condition is a new class, and they keep waiting
                     ;; for a standard one, as in SBCL.
                     (when (and found
                                (eq (class-of found) (find-class 'forward-referenced-class))
                                (eq metaclass (find-class 'condition-class)))
                       (setq found nil))
                     ;; A built-in or structure class is not redefined by
                     ;; `defclass`, as in SBCL, nor is a class turned into
                     ;; one of another kind: a condition class is redefined
                     ;; only by `define-condition` (`conditions.lisp`). A
                     ;; placeholder becomes whatever is defined in its name.
                     (when (and found
                                (not (eq (class-of found) (find-class 'forward-referenced-class)))
                                (not (eq (%metaclass-kind (class-of found))
                                         (%metaclass-kind metaclass))))
                       (error "defclass cannot redefine a class of this kind" name))
                     found))
         (class (progn
                  ;; A condition's parents must be defined already: SBCL
                  ;; refuses one that is not, and a placeholder made for
                  ;; it could later become a standard class.
                  (when (eq metaclass (find-class 'condition-class))
                    (dolist (super direct-superclasses)
                      (when (and (symbolp super) (null (find-class super nil)))
                        (error 'type-error "a condition's parent type is not defined" super))))
                  (or existing (%allocate-class-object (%class-wrapper metaclass)))))
         (supers (mapcar #'%superclass-named
                         (or direct-superclasses
                             (cond ((eq metaclass (find-class 'funcallable-standard-class))
                                    '(funcallable-standard-object))
                                   ((eq metaclass (find-class 'condition-class)) '(condition))
                                   (t '(standard-object))))))
         (replaced nil)
         (done nil)
         ;; What CLASS was, to put back if this does not finish.
         (old-wrapper (if existing (%instance-wrapper class) nil))
         (old-slots (if existing (%class-direct-slots class) nil))
         (old-initargs (if existing (%class-direct-default-initargs class) nil))
         (old-documentation (if existing (%class-documentation class) nil)))
    (when (member class supers)
      (error "a class cannot be its own superclass" name))
    ;; `stream` is a built-in class: a standard class may have it among its
    ;; superclasses only beside a standard class, as the Gray streams'
    ;; `fundamental-stream` has it beside `standard-object` (#136), or the
    ;; instances could not be made.
    (when (and (member (find-class 'stream) supers)
               (not (eq metaclass (find-class 'condition-class)))
               (notany (lambda (s) (%standard-metaclass-p (class-of s))) supers))
      (error "stream can be a superclass only beside a standard class" name))
    (dolist (super supers)
      ;; `condition` itself is the one condition class whose superclass
      ;; is `t` (`conditions.lisp`).
      (unless (or (%valid-superclass-p metaclass super)
                  (and (eq name 'condition) (eq super (find-class t))))
        (error "this class cannot be a superclass of a standard class" (%class-name super))))
    (%set-instance-wrapper class (%class-wrapper metaclass))
    (%set-class-name class name)
    (%set-class-documentation class documentation)
    (%set-class-direct-slots class (mapcar #'%make-direct-slot-definition direct-slots))
    (%prune-shared-cells class)
    (%set-class-direct-default-initargs class direct-default-initargs)
    (%invalidate-dispatch)
    (unwind-protect
         (progn
           (unless existing
             (setq replaced (gethash name %class-table))
             (setf (gethash name %class-table) class))
           (let ((old-supers (%class-direct-superclasses class)))
             (%set-superclasses class supers)
             (handler-case (%refinalize class)
               (error (condition)
                 (if existing
                     (%set-superclasses class old-supers)
                     (%set-superclasses class nil))
                 (error condition))))
           (%define-accessors class)
           (setq done t))
      (unless done
        (if existing
            (progn (%set-instance-wrapper class old-wrapper)
                   (%set-class-direct-slots class old-slots)
                   (%set-class-direct-default-initargs class old-initargs)
                   (%set-class-documentation class old-documentation))
            (if replaced
                (setf (gethash name %class-table) replaced)
                (remhash name %class-table)))))
    class))

;;; The metaclass `defclass` was given, as a class it can make an instance
;;; of: a standard class or one made from it, or the class of conditions.
;;; `built-in-class`, `structure-class`, an ordinary class and no class at
;;; all were taken, and made classes that could not make instances or that
;;; printed as `#<NIL>` (#189).
(defun %usable-metaclass (metaclass)
  (let ((class (if (and metaclass (symbolp metaclass)) (find-class metaclass) metaclass)))
    (unless (and class (%instance-wrapper class) (typep class 'class)
                 (or (eq class (find-class 'condition-class))
                     (member (find-class 'standard-class) (%class-precedence-list class))
                     (member (find-class 'funcallable-standard-class)
                             (%class-precedence-list class))))
      (error "defclass cannot make a class of this metaclass" metaclass))
    class))

;;; What kind of class METACLASS makes: `:standard` for the classes
;;; `defclass` makes, `:condition` for `define-condition`'s, nil for the
;;; rest, which neither makes nor redefines.
(defun %metaclass-kind (metaclass)
  (cond ((%standard-metaclass-p metaclass) :standard)
        ((eq metaclass (find-class 'condition-class)) :condition)
        (t nil)))

;;; Is METACLASS one whose instances `defclass` makes and redefines?
(defun %standard-metaclass-p (metaclass)
  (if (member metaclass (list (find-class 'standard-class)
                              (find-class 'funcallable-standard-class)
                              (find-class 'forward-referenced-class)))
      t
      nil))

;;; May SUPER be a direct superclass of a class of METACLASS? AMOP's
;;; `validate-superclass`: `t`, a standard or funcallable standard class
;;; (the two mix), a class not yet defined, and for a funcallable class
;;; also `function`. A built-in or structure class may not be: its
;;; instances are not standard objects.
(defun %valid-superclass-p (metaclass super)
  (if (eq metaclass (find-class 'condition-class))
      ;; A condition's superclasses are conditions (CLHS `define-condition`).
      (eq (class-of super) metaclass)
      (or (eq super (find-class t))
          (%standard-metaclass-p (class-of super))
          ;; `stream` too, as in SBCL, so that a stream a program defines
          ;; is a `stream` (Gray streams, `gray-streams.lisp`, #136).
          (and (not (eq metaclass (find-class 'funcallable-standard-class)))
               (eq super (find-class 'stream)))
          (and (eq metaclass (find-class 'funcallable-standard-class))
               (eq super (find-class 'function))))))

;;; A reader, writer or accessor named in a slot is a method of a generic
;;; function of that name (`generic-functions.lisp`). The methods a class's
;;; previous definition made are removed first, so a reader the new
;;; definition no longer names no longer applies; they are kept on the
;;; class's plist for that.
(defun %define-accessors (class)
  (dolist (method (getf (%class-plist class) '%accessor-methods))
    (when (%method-generic-function method)
      (remove-method (%method-generic-function method) method)))
  (let ((made nil))
    (dolist (slot (%class-direct-slots class))
      (let ((name (%slot-definition-name slot)))
        (dolist (reader (%slot-definition-readers slot))
          (push (%add-reader-method class reader name) made))
        (dolist (writer (%slot-definition-writers slot))
          (push (%add-writer-method class writer name) made))))
    (%set-class-plist class (list* '%accessor-methods made
                                   (%remove-plist-key (%class-plist class) '%accessor-methods)))))

;;; A slot specifier of `defclass` as the list of keywords `ensure-class`
;;; takes: (:name x :initargs (:x) :initform 0 :initfunction (lambda () 0) ...).
;;; The initform is wrapped in a function made where the `defclass` is, so
;;; that it sees that lexical environment.
(defun %canonical-slot (spec)
  (let* ((spec (if (symbolp spec) (list spec) spec))
         (name (car spec))
         (options (cdr spec))
         (initargs nil) (readers nil) (writers nil)
         (initform nil) (initform-p nil)
         (allocation nil) (type nil) (documentation nil))
    (unless (and (symbolp name) name) (error 'program-error "a slot name must be a symbol" name))
    (unless (evenp (length options)) (error 'program-error "malformed slot options" spec))
    (do ((rest options (cddr rest)))
        ((null rest))
      (let ((key (car rest)) (value (cadr rest)))
        (case key
          (:initarg (push value initargs))
          (:initform (when initform-p (error 'program-error "two :initform options" spec))
                     (setq initform value initform-p t))
          (:reader (push value readers))
          (:writer (push value writers))
          (:accessor (push value readers) (push (list 'setf value) writers))
          (:allocation (when allocation (error 'program-error "two :allocation options" spec))
                       (setq allocation value))
          (:type (when type (error 'program-error "two :type options" spec))
                 (setq type value))
          (:documentation (when documentation (error 'program-error "two :documentation options" spec))
                          (setq documentation value))
          (t (error 'program-error "an unknown slot option" key)))))
    `(list :name ',name :initargs ',(nreverse initargs)
           ,@(if initform-p `(:initform ',initform :initfunction (lambda () ,initform)) nil)
           :allocation ,(or allocation :instance) :type ',(or type t)
           ;; Readers and writers newest first, as SBCL lists them.
           :readers ',readers :writers ',writers
           :documentation ,documentation)))

;;; The class options of `defclass` as keywords for `ensure-class`.
(defun %canonical-class-options (options)
  (let ((result nil) (seen nil))
    (dolist (option options (nreverse result))
      ;; Each class option may be given once (CLHS `defclass`).
      (when (member (car option) seen)
        (error 'program-error "a defclass option given twice" (car option)))
      (push (car option) seen)
      (case (car option)
        (:default-initargs
         (let ((entries nil) (keys nil))
           (do ((rest (cdr option) (cddr rest)))
               ((null rest))
             (when (member (car rest) keys)
               (error 'program-error "a default initarg given twice" (car rest)))
             (push (car rest) keys)
             (push `(list ',(car rest) ',(cadr rest) (lambda () ,(cadr rest))) entries))
           (push :direct-default-initargs result)
           (push `(list ,@(nreverse entries)) result)))
        (:metaclass (push :metaclass result) (push `',(cadr option) result))
        (:documentation (push :documentation result) (push (cadr option) result))
        (t (error 'program-error "an unknown class option" (car option)))))))

(defmacro defclass (name direct-superclasses direct-slots &rest options)
  (let ((names (mapcar (lambda (spec) (if (consp spec) (car spec) spec)) direct-slots)))
    (unless (= (length names) (length (remove-duplicates names)))
      (error 'program-error "two slots with the same name" name)))
  `(ensure-class ',name
                 :direct-superclasses ',direct-superclasses
                 :direct-slots (list ,@(mapcar #'%canonical-slot direct-slots))
                 ,@(%canonical-class-options options)))

;;; ------------------------------------------------------------------------
;;; Making instances (CLHS 7.1)

;;; The class a class designator names.
(defun %class-designated (class)
  (if (symbolp class) (find-class class) class))

;;; The worker every way of making an instance comes to, and what the
;;; standard methods of `allocate-instance` call (`generic-functions.lisp`).
;;;
;;; The instance protocol's public functions (`make-instance`,
;;; `allocate-instance`, `initialize-instance`, `reinitialize-instance`,
;;; `shared-initialize`, `slot-missing`, `slot-unbound`) are generic
;;; functions, defined in `generic-functions.lisp`; this file has only the
;;; workers their standard methods call. Were they defined here as ordinary
;;; functions, a warm rebuild would replace each generic function with one
;;; before `generic-functions.lisp` made it generic again, and the generic
;;; function object, with any method a program had added, would be lost.
(defun %allocate-instance (class)
  (let* ((class (%ensure-finalized (%class-designated class)))
         (wrapper (%class-wrapper class))
         ;; `%make-vector`, the primitive under `make-array`: an instance is
         ;; made on every `make-instance` and every condition signalled,
         ;; and `make-array`'s keyword arguments cost more than the rest.
         (slots (%make-vector (length (%wrapper-slot-names wrapper)) %slot-unbound-marker)))
    (if (eq (class-of class) (find-class 'funcallable-standard-class))
        (%allocate-funcallable-instance wrapper slots)
        (%allocate-standard-instance wrapper slots))))

;;; INITARGS with the class's default initargs that it does not supply
;;; added at the end, each initform evaluated now.
(defun %add-default-initargs (class initargs)
  (let ((added nil))
    (dolist (entry (%class-default-initargs class))
      (unless (%plist-has-key initargs (car entry))
        (push (car entry) added)
        (push (funcall (third entry)) added)))
    (append initargs (nreverse added))))

(defun %plist-has-key (plist key)
  (do ((rest plist (cddr rest)))
      ((null rest) nil)
    (when (eq (car rest) key) (return t))))

;;; Check INITARGS against what CLASS accepts (CLHS 7.1.2): the slots'
;;; initargs, `:allow-other-keys`, and the keywords the applicable methods
;;; of the generic functions named in FUNCTIONS accept, unless
;;; `:allow-other-keys` is true or one of those methods allows any.
(defun %check-initargs (class initargs functions &optional previous)
  (unless (and (listp initargs) (evenp (length initargs)))
    (error 'program-error "an odd number of initialization arguments" initargs))
  ;; `change-class` passes the previous class too: rare enough to need no
  ;; cache.
  (let ((valid (if previous
                   (%compute-valid-initargs class functions previous)
                   (%valid-initargs class functions))))
    (unless (or (eq valid t) (getf initargs :allow-other-keys))
      (do ((rest initargs (cddr rest)))
          ((null rest))
        (unless (%member-eq (car rest) valid)
          (error 'program-error "an invalid initialization argument" (car rest)))))))

;;; The initargs CLASS accepts for FUNCTIONS, or t when it accepts any.
(defun %compute-valid-initargs (class functions previous)
  (multiple-value-bind (method-keys any)
      (%compute-method-initargs class functions previous)
    (if any
        t
        (let ((valid (cons :allow-other-keys method-keys)))
          (dolist (slot (%class-slots class) valid)
            (setq valid (append (%slot-definition-initargs slot) valid)))))))

;;; What the standard method of `make-instance` does (CLHS 7.1): the default
;;; initargs added, the initargs checked, then `allocate-instance` and
;;; `initialize-instance`, both generic.
(defun %make-instance (class &rest initargs)
  (let* ((class (%ensure-finalized (%class-designated class)))
         (initargs (%add-default-initargs class initargs)))
    (unless (or (eq (class-of class) (gethash 'standard-class %class-table))
                (eq (class-of class) (gethash 'funcallable-standard-class %class-table)))
      (error "make-instance cannot make an instance of this class" (%class-name class)))
    (%check-initargs class initargs '(allocate-instance initialize-instance shared-initialize))
    ;; When only the standard methods of `allocate-instance`,
    ;; `initialize-instance` and `shared-initialize` apply to the class,
    ;; what they would do is done here without the three dispatches, which
    ;; cost more than the work (#163). PCL calls this a constructor
    ;; optimisation.
    (if (%plain-initialization-p class)
        (let ((instance (%allocate-instance class)))
          (apply #'%shared-initialize instance t initargs)
          instance)
        (let ((instance (apply #'allocate-instance class initargs)))
          (apply #'initialize-instance instance initargs)
          instance))))

(defun %reinitialize-instance (instance &rest initargs)
  (%ensure-current-instance instance)
  (%check-initargs (class-of instance) initargs '(reinitialize-instance shared-initialize))
  (apply #'shared-initialize instance nil initargs)
  instance)

;;; Fill INSTANCE's slots: each from the first initarg that names it, and
;;; otherwise, if it is unbound and one of SLOT-NAMES (or SLOT-NAMES is t),
;;; from its initform. What the standard method of `shared-initialize` does.
(defun %shared-initialize (instance slot-names &rest initargs)
  (dolist (slot (%class-slots (class-of instance)) instance)
    (let ((name (%slot-definition-name slot))
          (found nil) (value nil))
      (do ((rest initargs (cddr rest)))
          ((null rest))
        (when (%member-eq (car rest) (%slot-definition-initargs slot))
          (setq found t value (cadr rest))
          (return)))
      (cond (found (%set-slot-value-using-slot instance slot value))
            ((and (or (eq slot-names t) (member name slot-names))
                  (%slot-definition-initfunction slot)
                  (eq (%slot-value-using-slot instance slot) %slot-unbound-marker))
             (%set-slot-value-using-slot instance slot
                                         (funcall (%slot-definition-initfunction slot))))))))

;;; ------------------------------------------------------------------------
;;; Slots (CLHS 7.5)

;;; What a slot holds, the unbound marker included: an instance slot from
;;; the slot vector, a class slot from its shared cons.
(defun %slot-value-using-slot (instance slot)
  (let ((location (%slot-definition-location slot)))
    (if (consp location)
        (cdr location)
        (aref (%instance-slots instance) location))))

(defun %set-slot-value-using-slot (instance slot value)
  (let ((location (%slot-definition-location slot)))
    (if (consp location)
        (setf (cdr location) value)
        (setf (aref (%instance-slots instance) location) value))))

;;; Where the instance OBJECT keeps its local slot named NAME: an index
;;; into its slot vector, or nil when it has no such local slot (a shared
;;; slot, no slot of that name, or no instance at all). A wrapper lists the
;;; names of the local slots in the order the slot vector keeps them, so a
;;; name's place in that list is its slot's location, and nothing has to
;;; find the slot definition. This is the path nearly every slot access
;;; takes: finding the definition with `find` and `:key` cost 640 steps for
;;; a first slot and 1,800 for a tenth (#162).
(defun %local-slot-index (object name)
  (and (%instance-wrapper object)
       (progn
         (%ensure-current-instance object)
         (let* ((names (%wrapper-slot-names (%instance-wrapper object)))
                (count (length names)))
           (do ((i 0 (+ i 1)))
               ((or (= i count) (eq name (aref names i)))
                (if (= i count) nil i)))))))

;;; OBJECT's effective slot named NAME, or nil.
(defun %find-slot (object name)
  (if (%instance-wrapper object)
      (progn (%ensure-current-instance object)
             (find name (%class-slots (class-of object)) :key #'%slot-definition-name))
      nil))

;;; An instance made before its class was redefined with other slots, or
;;; made obsolete, still has the old wrapper and a slot vector laid out by
;;; it. The first time one of its slots is touched it is brought up to date
;;; (CLHS 4.3.6): a new slot vector in the class's current layout, keeping
;;; the value of every slot that was local before or shared before. The
;;; update is then told, as SBCL tells it (measured):
;;;
;;; - added: every local slot still unbound after the copy, one that was
;;;   kept but unbound included, so that it gets its initform (CLHS
;;;   4.3.6.1 would leave that one unbound);
;;; - discarded: every local slot that went and had a value;
;;; - the property list: those slots' names and values;
;;;
;;; each in reverse slot order. The instance gets its new wrapper and
;;; vector first, so that `update-instance-for-redefined-class`, the generic
;;; function a program can specialise, sees the new layout; its standard
;;; method fills the added slots from their initforms. If the update does
;;; not finish, the instance gets its old wrapper and vector back, as in
;;; SBCL, so the next touch tries again. CLHS leaves *when* the update
;;; happens to the implementation: SBCL also does it on a generic function
;;; call, Quokka only here, keeping dispatch as cheap.
;;;
;;; A class that is not finalized just now (a superclass was redefined to
;;; one not defined yet) still has the wrapper and slots it was last
;;; finalized with, and the instance is brought up to that.
(defun %ensure-current-instance (instance)
  (let* ((wrapper (%instance-wrapper instance))
         (class (%wrapper-class wrapper))
         (current (%class-wrapper class)))
    (unless (or (eq wrapper current) (null current))
      (let* ((old-names (%wrapper-slot-names wrapper))
             (old-slots (%instance-slots instance))
             (old-cells (cdr (%wrapper-state wrapper)))
             (new-names (%wrapper-slot-names current))
             (new-slots (make-array (length new-names) :initial-element %slot-unbound-marker))
             (added nil) (discarded nil) (plist nil) (done nil))
        (dotimes (i (length new-names))
          (let* ((name (aref new-names i))
                 (at (position name old-names))
                 (old-cell (assoc name old-cells)))
            (cond (at (setf (aref new-slots i) (aref old-slots at)))
                  (old-cell (setf (aref new-slots i) (cdr (cdr old-cell)))))
            (when (eq (aref new-slots i) %slot-unbound-marker) (push name added))))
        (dotimes (i (length old-names))
          (let ((name (aref old-names i))
                (value (aref old-slots i)))
            (unless (or (find name new-names) (eq value %slot-unbound-marker))
              (push name discarded)
              (push value plist)
              (push name plist))))
        (%set-instance-slots instance new-slots)
        (%set-instance-wrapper instance current)
        (unwind-protect
             (progn (update-instance-for-redefined-class instance added discarded plist)
                    (setq done t))
          (unless done
            (%set-instance-slots instance old-slots)
            (%set-instance-wrapper instance wrapper)))))
    instance))

;;; What the standard methods of `change-class` do (CLHS 7.2): INSTANCE,
;;; brought up to date first, gets NEW-CLASS's wrapper and a slot vector in
;;; its layout, keeping the value of every slot that was local before or
;;; shared before, and stays the same object. PREVIOUS, a copy of it as it
;;; was, with the old class and slots, is handed to
;;; `update-instance-for-different-class` with it. A standard instance and
;;; a funcallable one are different objects in the heap, so neither becomes
;;; the other, as in SBCL.
(defun %change-class (instance new-class initargs)
  (%ensure-current-instance instance)
  (unless (eq (if (%funcallable-instance-p instance) t nil)
              (eq (class-of new-class) (find-class 'funcallable-standard-class)))
    (error "change-class cannot make a funcallable instance of a standard class, or the reverse"
           instance))
  (let* ((new-class (%ensure-finalized new-class))
         (old-wrapper (%instance-wrapper instance))
         (old-names (%wrapper-slot-names old-wrapper))
         (old-slots (%instance-slots instance))
         (old-cells (%class-shared-cells (%wrapper-class old-wrapper)))
         ;; A funcallable instance's copy is funcallable too, and calls
         ;; what the instance calls.
         (previous (if (%funcallable-instance-p instance)
                       (let ((copy (%allocate-funcallable-instance old-wrapper (copy-seq old-slots))))
                         (%set-funcallable-instance-function
                          copy (lambda (&rest arguments) (apply instance arguments)))
                         copy)
                       (%allocate-standard-instance old-wrapper (copy-seq old-slots))))
         (new-wrapper (%class-wrapper new-class))
         (new-names (%wrapper-slot-names new-wrapper))
         (new-slots (make-array (length new-names) :initial-element %slot-unbound-marker)))
    (dotimes (i (length new-names))
      (let* ((name (aref new-names i))
             (at (position name old-names))
             (old-cell (assoc name old-cells)))
        (cond (at (setf (aref new-slots i) (aref old-slots at)))
              (old-cell (setf (aref new-slots i) (cdr (cdr old-cell)))))))
    (%set-instance-slots instance new-slots)
    (%set-instance-wrapper instance new-wrapper)
    ;; A change whose update does not finish is undone, as in SBCL.
    (let ((done nil))
      (unwind-protect
           (progn (apply #'update-instance-for-different-class previous instance initargs)
                  (setq done t))
        (unless done
          (%set-instance-slots instance old-slots)
          (%set-instance-wrapper instance old-wrapper))))
    instance))

;;; The local slots of CURRENT's class that PREVIOUS's class had no slot of
;;; the same name for: those `update-instance-for-different-class` fills.
(defun %slots-added-by-change (previous current)
  (let ((old (mapcar #'%slot-definition-name (%class-slots (class-of previous))))
        (added nil))
    (dolist (slot (%class-slots (class-of current)) (nreverse added))
      (unless (or (consp (%slot-definition-location slot))
                  (member (%slot-definition-name slot) old))
        (push (%slot-definition-name slot) added)))))

;;; A structure's slot description named NAME, for `slot-value` on a
;;; structure, which SBCL allows.
(defun %structure-slot (object name)
  (let ((layout (%structure-layout object)))
    (and layout (find name (%layout-slots layout) :key #'car))))

(defun slot-value (object slot-name)
  (let* ((index (%local-slot-index object slot-name))
         (slot (if index nil (%find-slot object slot-name))))
    (cond ((or index slot)
           (let ((value (if index
                            (aref (%instance-slots object) index)
                            (%slot-value-using-slot object slot))))
             ;; Only the first value of `slot-unbound` and `slot-missing`
             ;; (CLHS `slot-value`).
             (if (eq value %slot-unbound-marker)
                 (values (slot-unbound (class-of object) object slot-name))
                 value)))
          ((%structure-slot object slot-name)
           (%structure-ref object (second (%structure-slot object slot-name))))
          (t (values (slot-missing (class-of object) object slot-name 'slot-value))))))

(defun (setf slot-value) (new object slot-name)
  (let* ((index (%local-slot-index object slot-name))
         (slot (if index nil (%find-slot object slot-name))))
    (cond (index (setf (aref (%instance-slots object) index) new))
          (slot (%set-slot-value-using-slot object slot new))
          ((%structure-slot object slot-name)
           (%set-structure-ref object (second (%structure-slot object slot-name)) new))
          (t (slot-missing (class-of object) object slot-name 'setf new) new))))

(defun slot-boundp (object slot-name)
  (let* ((index (%local-slot-index object slot-name))
         (slot (if index nil (%find-slot object slot-name))))
    (cond (index (not (eq (aref (%instance-slots object) index) %slot-unbound-marker)))
          (slot (not (eq (%slot-value-using-slot object slot) %slot-unbound-marker)))
          ((%structure-slot object slot-name) t)
          ;; The truth of what `slot-missing` returns (CLHS `slot-boundp`).
          (t (if (slot-missing (class-of object) object slot-name 'slot-boundp) t nil)))))

(defun slot-makunbound (object slot-name)
  (let ((slot (%find-slot object slot-name)))
    (if slot
        (%set-slot-value-using-slot object slot %slot-unbound-marker)
        (slot-missing (class-of object) object slot-name 'slot-makunbound))
    object))

(defun slot-exists-p (object slot-name)
  (if (or (%find-slot object slot-name) (%structure-slot object slot-name)) t nil))

;;; What the standard methods of the generic functions `slot-missing` and
;;; `slot-unbound` do: signal an error.
(defun %slot-missing (object slot-name)
  (error "the object has no slot of this name" (list object slot-name)))

(defun %slot-unbound (object slot-name)
  ;; With CLHS's initargs, so a handler can ask which slot of what.
  (error 'unbound-slot :name slot-name :instance object))

;;; ------------------------------------------------------------------------
;;; with-slots, with-accessors (sub-step 4)
;;;
;;; Each makes names that stand for places — a slot of an instance, or what
;;; an accessor reads — by `symbol-macrolet` (CLHS 3.1.2.1.1.3): `x` in the
;;; body is replaced by `(slot-value instance 'x)`, so reading it reads the
;;; slot and `setq`, `setf` and `incf` of it write the slot. The instance
;;; form is evaluated once, into a variable of its own.

(defmacro with-slots (slot-entries instance-form &body body)
  (let ((instance (gensym "INSTANCE")))
    `(let ((,instance ,instance-form))
       (symbol-macrolet ,(mapcar (lambda (entry)
                                   (if (consp entry)
                                       `(,(first entry) (slot-value ,instance ',(second entry)))
                                       `(,entry (slot-value ,instance ',entry))))
                                 slot-entries)
         ,@body))))

(defmacro with-accessors (slot-entries instance-form &body body)
  (let ((instance (gensym "INSTANCE")))
    `(let ((,instance ,instance-form))
       (symbol-macrolet ,(mapcar (lambda (entry) `(,(first entry) (,(second entry) ,instance)))
                                 slot-entries)
         ,@body))))
