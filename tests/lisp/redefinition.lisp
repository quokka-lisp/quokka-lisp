;;; Redefinition (phase 3, CLOS sub-step 5, #124): obsolete instances,
;;; update-instance-for-redefined-class, make-instances-obsolete, shared
;;; slots, change-class. Tier 1: compiled and SBCL only. CLHS 4.3.6 leaves
;;; *when* an obsolete instance is updated to the implementation (SBCL also
;;; updates on a generic function call, Quokka when a slot is touched), so
;;; every log is read after a slot-value.

(defvar *log* nil)
(defun take-log () (prog1 (reverse *log*) (setq *log* nil)))

;; A redefinition with the same slots makes nothing obsolete.
(class-name (defclass rp () ((x :initarg :x :initform 1) (y :initarg :y :initform 2))))
(progn (defmethod update-instance-for-redefined-class :before ((i rp) added discarded plist &rest initargs)
         (push (list :update added discarded plist initargs) *log*))
       :ok)
(defvar *rp* (make-instance 'rp :x 10))
(class-name (defclass rp () ((x :initarg :x :initform 1) (y :initarg :y :initform 3))))
(list (slot-value *rp* 'x) (slot-value *rp* 'y) (take-log))

;; A slot removed and one added: the update is told which, with the old
;; value of the bound one that went.
(class-name (defclass rp () ((x :initarg :x :initform 1) (z :initform 99))))
(list (slot-value *rp* 'x) (slot-value *rp* 'z) (take-log))
(slot-exists-p *rp* 'y)

;; An unbound slot that goes is not in the property list.
(class-name (defclass rq () ((a :initarg :a) (b :initarg :b))))
(progn (defmethod update-instance-for-redefined-class :before ((i rq) added discarded plist &rest initargs)
         initargs (push (list added discarded plist) *log*))
       :ok)
(defvar *rq* (make-instance 'rq :a 1))
(class-name (defclass rq () ()))
(list (slot-exists-p *rq* 'a) (take-log))

;; make-instances-obsolete returns its argument, and the next update has
;; nothing added or discarded.
(make-instances-obsolete 'rp)
(list (slot-value *rp* 'z) (take-log))
(class-name (make-instances-obsolete (find-class 'rp)))
(list (slot-value *rp* 'x) (take-log))
(handler-case (make-instances-obsolete 'no-such-class-here) (error () :error))

;; Two redefinitions before the instance is touched: one update, between
;; its own old slots and the current ones.
(defvar *rp2* (make-instance 'rp :x 5))
(class-name (defclass rp () ((x :initarg :x :initform 1) (z :initform 99) (w :initform 4))))
(class-name (defclass rp () ((x :initarg :x :initform 1) (w :initform 4))))
(list (slot-value *rp2* 'x) (slot-value *rp2* 'w) (take-log))

;; An :after method that reads the instance it is updating.
(progn (defmethod update-instance-for-redefined-class :after ((i rp) added discarded plist &rest initargs)
         added discarded plist initargs
         (push (list :after (slot-value i 'x)) *log*))
       :ok)
(class-name (defclass rp () ((x :initarg :x :initform 1) (w :initform 4) (v :initform 0))))
(list (slot-value *rp2* 'v) (take-log))

;; A subclass's instances are updated when a superclass loses or gains a
;; slot, and an accessor finds its slot at its new place.
(class-name (defclass base () ((a :initarg :a :accessor base-a) (b :initarg :b :initform :b))))
(class-name (defclass derived (base) ((c :initarg :c :initform :c :accessor derived-c))))
(defvar *derived* (make-instance 'derived :a 1))
(class-name (defclass base () ((n :initform :new) (a :initarg :a :accessor base-a))))
(list (base-a *derived*) (derived-c *derived*) (slot-value *derived* 'n) (slot-exists-p *derived* 'b))

;; Shared slots: initialised when the class is defined, kept when it is
;; redefined, and one defined again in a subclass is its own.
(class-name (defclass se () ((a :allocation :class :initform 5))))
(slot-value (allocate-instance (find-class 'se)) 'a)
(class-name (defclass se () ((a :allocation :class :initform 50) (b :allocation :class :initform 6))))
(list (slot-value (allocate-instance (find-class 'se)) 'a) (slot-value (allocate-instance (find-class 'se)) 'b))
(class-name (defclass sf (se) ((a :allocation :class :initform 500))))
(list (slot-value (allocate-instance (find-class 'sf)) 'a) (slot-value (allocate-instance (find-class 'se)) 'a))

;; Shared to local keeps the value; local to shared takes the shared one,
;; and the local value goes to the property list.
(class-name (defclass sl () ((p :allocation :class :initform :pc) (q :initform :q1))))
(progn (defmethod update-instance-for-redefined-class :before ((i sl) added discarded plist &rest initargs)
         initargs (push (list added discarded plist) *log*))
       :ok)
(defvar *sl* (make-instance 'sl))
(class-name (defclass sl () ((p :initform :new-p) (q :allocation :class :initform :q2))))
(list (slot-value *sl* 'p) (slot-value *sl* 'q) (take-log))
(setf (slot-value (make-instance 'sl) 'p) :other)
(slot-value *sl* 'p)

;; change-class: the same object, of the new class; slots local in both
;; keep their values, a slot shared before and local now keeps its value,
;; and initargs fill slots. A :before method sees the previous instance
;; with the old class and values, and the new slots still unbound.
(class-name (defclass ca () ((x :initarg :x :initform 1) (s :allocation :class :initform :shared-a)
                             (y :initarg :y))))
(class-name (defclass cb () ((x :initarg :x :initform 5) (w :initarg :w :initform 7)
                             (s :initform :local-b))))
(progn (defmethod update-instance-for-different-class :before ((old ca) (new cb) &rest initargs)
         (push (list (class-name (class-of old)) (slot-value old 'x) (slot-boundp old 'y)
                     (class-name (class-of new)) (slot-boundp new 'w) initargs)
               *log*))
       :ok)
(defvar *ca* (make-instance 'ca :x 10))
(let ((changed (change-class *ca* 'cb :w 70)))
  (list (eq changed *ca*) (class-name (class-of *ca*)) (slot-value *ca* 'x) (slot-value *ca* 'w)
        (slot-value *ca* 's) (take-log)))
(typep *ca* 'cb)
(typep *ca* 'ca)

;; A method on the new class applies afterwards.
(progn (defgeneric kind (o)) (defmethod kind ((o ca)) :ca) (defmethod kind ((o cb)) :cb) :ok)
(let ((i (make-instance 'ca))) (list (kind i) (progn (change-class i (find-class 'cb)) (kind i))))

;; Added slots get their initforms; an invalid initarg is a program-error.
(let ((i (make-instance 'ca))) (change-class i 'cb) (list (slot-value i 'x) (slot-value i 'w)))
(handler-case (change-class (make-instance 'ca) 'cb :bogus 1) (program-error () :program-error))
(take-log)

;; What change-class refuses.
(handler-case (change-class 5 'cb) (error () :error))
(defstruct cs a)
(handler-case (change-class (make-cs) 'cb) (error () :error))
(handler-case (change-class (make-instance 'ca) 'no-such-class-here) (error () :error))

;; To the class it already has.
(let ((i (make-instance 'cb :w 3))) (list (eq i (change-class i 'cb)) (slot-value i 'w)))

;; An obsolete instance is brought up to date before it is changed.
(class-name (defclass cc () ((m :initarg :m) (gone :initform :g))))
(defvar *cc* (make-instance 'cc :m 1))
(class-name (defclass cc () ((m :initarg :m) (n :initform :n))))
(class-name (defclass cd () ((m :initarg :m) (n :initform :other) (o :initform :o))))
(progn (change-class *cc* 'cd) (list (slot-value *cc* 'm) (slot-value *cc* 'n) (slot-value *cc* 'o)))

;; What the update calls added: every new local slot still unbound after
;; the copy, an unbound one kept included (SBCL; CLHS 4.3.6.1 would keep
;; it unbound), and all three lists in reverse slot order, as SBCL gives
;; them.
(class-name (defclass mo () ((a) (b :initform 2) (z) (y :initform 9))))
(progn (defmethod update-instance-for-redefined-class :before ((i mo) added discarded plist &rest initargs)
         initargs (push (list added discarded plist) *log*))
       :ok)
(defvar *mo* (make-instance 'mo))
(class-name (defclass mo () ((a :initform 1) (b :initform 2) (c :initform 3) (d :initform 4))))
(list (slot-value *mo* 'a) (slot-value *mo* 'b) (slot-value *mo* 'd) (take-log))
(class-name (defclass mn () ((a :initarg :a) (b :initarg :b) (c :initform 3))))
(progn (defmethod update-instance-for-redefined-class :before ((i mn) added discarded plist &rest initargs)
         initargs (push (list added discarded plist) *log*))
       :ok)
(defvar *mn* (make-instance 'mn :a 1 :b 2))
(class-name (defclass mn () ((c :initform 3))))
(list (slot-value *mn* 'c) (take-log))
(class-name (defclass mi () ((a :initarg :a) (b))))
(progn (defmethod update-instance-for-redefined-class :before ((i mi) added discarded plist &rest initargs)
         initargs (push (list added discarded plist) *log*))
       :ok)
(defvar *mi* (make-instance 'mi))
(make-instances-obsolete 'mi)
(list (slot-boundp *mi* 'a) (take-log))

;; An update that fails leaves the instance as it was, so the next touch
;; tries again.
(class-name (defclass er () ((a :initform 1))))
(defvar *er* (make-instance 'er))
(defvar *fail* t)
(progn (defmethod update-instance-for-redefined-class :before ((i er) added discarded plist &rest initargs)
         added discarded plist initargs (when *fail* (error "the update fails")))
       :ok)
(class-name (defclass er () ((a :initform 1) (b :initform 2))))
(handler-case (slot-value *er* 'b) (error () :error))
(setq *fail* nil)
(list (slot-boundp *er* 'b) (slot-value *er* 'a))

;; So does a change-class whose update fails.
(class-name (defclass ce1 () ((a :initform 1))))
(class-name (defclass ce2 () ((a :initform 1) (b :initform 2))))
(progn (defmethod update-instance-for-different-class :before ((old ce1) (new ce2) &rest initargs)
         initargs (when *fail* (error "the change fails")))
       :ok)
(defvar *ce* (make-instance 'ce1))
(setq *fail* t)
(handler-case (change-class *ce* 'ce2) (error () :error))
(list (class-name (class-of *ce*)) (slot-exists-p *ce* 'b) (slot-value *ce* 'a))
(setq *fail* nil)
