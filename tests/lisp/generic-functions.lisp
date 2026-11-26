;;; Generic functions (phase 3, step A3, #124): defgeneric, defmethod,
;;; dispatch, standard method combination. Tier 1: compiled and SBCL only.
;;; A generic function prints with an address in SBCL, so the file never
;;; prints one.

(class-name (defclass shape () ()))
(class-name (defclass circle (shape) ((r :initarg :r :reader r))))
(class-name (defclass square (shape) ((side :initarg :side :reader side))))
(class-name (defclass small-circle (circle) ()))

;; Methods on classes, and the most specific one chosen.
(progn (defgeneric describe-shape (s)) :defined)
(progn (defmethod describe-shape ((s shape)) :shape) :ok)
(progn (defmethod describe-shape ((s circle)) (list :circle (call-next-method))) :ok)
(progn (defmethod describe-shape ((s small-circle)) (list :small (call-next-method))) :ok)
(describe-shape (make-instance 'square))
(describe-shape (make-instance 'circle))
(describe-shape (make-instance 'small-circle))
(handler-case (describe-shape 5) (error () :no-method))

;; Methods on built-in classes and on t.
(progn (defmethod kind ((x integer)) :integer) (defmethod kind ((x string)) :string)
       (defmethod kind ((x null)) :null) (defmethod kind ((x list)) :list)
       (defmethod kind ((x t)) :other) :ok)
(list (kind 1) (kind "a") (kind nil) (kind '(1)) (kind 'a) (kind 1.5))

;; A method on a structure type.
(defstruct gpoint x)
(progn (defmethod kind ((x gpoint)) :gpoint) :ok)
(kind (make-gpoint))

;; eql specializers win over classes.
(progn (defmethod kind ((x (eql 7))) :seven) :ok)
(list (kind 7) (kind 8))
(progn (defmethod kind ((x (eql :key))) :the-key) :ok)
(list (kind :key) (kind :other))

;; next-method-p, and call-next-method with new arguments.
(progn (defmethod nm ((x integer)) (list :integer x (next-method-p) (call-next-method (* x 10)))) :ok)
(progn (defmethod nm ((x number)) (list :number x (next-method-p))) :ok)
(nm 3)
(handler-case (nm 1.5) (error () :error))
(progn (defmethod lonely ((x integer)) (call-next-method)) :ok)
(handler-case (lonely 1) (error () :no-next))

;; Before, after and around methods.
(defvar *trace* nil)
(progn (defmethod act ((x circle)) (push :primary-circle *trace*) :result) :ok)
(progn (defmethod act ((x shape)) (push :primary-shape *trace*) :never) :ok)
(progn (defmethod act :before ((x circle)) (push :before-circle *trace*)) :ok)
(progn (defmethod act :before ((x shape)) (push :before-shape *trace*)) :ok)
(progn (defmethod act :after ((x circle)) (push :after-circle *trace*)) :ok)
(progn (defmethod act :after ((x shape)) (push :after-shape *trace*)) :ok)
(progn (defmethod act :around ((x shape)) (push :around-in *trace*) (let ((v (call-next-method))) (push :around-out *trace*) (list :around v))) :ok)
(list (act (make-instance 'circle)) (reverse *trace*))

;; Multiple values pass through every kind of method.
(progn (defmethod two ((x t)) (values 1 2)) (defmethod two :around ((x t)) (call-next-method)) (defmethod two :after ((x t)) :ignored) :ok)
(multiple-value-list (two 0))

;; Multiple dispatch: both arguments, the leftmost breaking a tie.
(progn (defmethod meet ((a circle) (b shape)) :circle-shape)
       (defmethod meet ((a shape) (b circle)) :shape-circle)
       (defmethod meet ((a shape) (b shape)) :shape-shape) :ok)
(list (meet (make-instance 'circle) (make-instance 'square))
      (meet (make-instance 'square) (make-instance 'circle))
      (meet (make-instance 'circle) (make-instance 'circle))
      (meet (make-instance 'square) (make-instance 'square)))

;; Too few arguments.
(handler-case (meet (make-instance 'circle)) (program-error () :program-error))

;; Redefining a method replaces it; remove-method and find-method.
(progn (defmethod kind ((x integer)) :integer-again) :ok)
(kind 1)
(progn (remove-method #'kind (find-method #'kind nil (list (find-class 'integer)))) :ok)
(kind 1)
(find-method #'kind nil (list (find-class 'integer)) nil)
(handler-case (find-method #'kind nil (list (find-class 'integer))) (error () :not-found))
(method-qualifiers (find-method #'act '(:before) (list (find-class 'circle))))

;; A generic function is a function.
(list (typep #'kind 'generic-function) (typep #'kind 'standard-generic-function) (typep #'kind 'function)
      (functionp #'kind) (compiled-function-p #'kind))
(class-name (class-of #'kind))
(funcall #'kind "x")
(apply #'kind '(:key))
(mapcar #'kind '(1 "a" nil))

;; A method not congruent with its generic function is an error.
(handler-case (defmethod describe-shape ((s shape) extra) extra) (error () :not-congruent))
(describe-shape (make-instance 'circle))

;; A class redefined after it was dispatched on.
(class-name (defclass base-a () ()))
(class-name (defclass base-b () ()))
(class-name (defclass mixed (base-a) ()))
(progn (defmethod which ((x base-a)) :a) (defmethod which ((x base-b)) :b) :ok)
(which (make-instance 'mixed))
(class-name (defclass mixed (base-b) ()))
(which (make-instance 'mixed))

;; call-next-method kept in a closure and called after the method returned.
(progn (defmethod later ((x integer)) (lambda () (call-next-method))) (defmethod later ((x number)) (list :number x)) :ok)
(funcall (later 4))

;; defgeneric and its options.
(progn (defgeneric greet (x) (:documentation "Say hello.") (:method ((x string)) (list :hello x)) (:method ((x integer)) (list :number x))) :ok)
(list (greet "you") (greet 3))
(progn (defgeneric greet (x) (:method ((x string)) (list :hi x))) :ok)
(greet "you")
(handler-case (greet 3) (error () :gone))
(typep (defgeneric fresh-gf (a b)) 'generic-function)
(handler-case (defmethod fresh-gf ((a t)) a) (error () :not-congruent))

;; Argument precedence order.
(progn (defgeneric order-test (a b) (:argument-precedence-order b a)
         (:method ((a circle) (b shape)) :first-specific)
         (:method ((a shape) (b circle)) :second-specific)) :ok)
(order-test (make-instance 'circle) (make-instance 'circle))

;; Optional and keyword parameters.
(progn (defgeneric opt (x &optional y)) (defmethod opt ((x integer) &optional (y 10)) (+ x y)) :ok)
(list (opt 1) (opt 1 2))
(progn (defgeneric keyed (x &key)) :ok)
(progn (defmethod keyed ((x shape) &key size) (list :shape size)) :ok)
(progn (defmethod keyed ((x circle) &key colour) (list :circle colour (call-next-method))) :ok)
(keyed (make-instance 'circle) :colour 'red :size 2)
(keyed (make-instance 'square) :size 3)
(handler-case (keyed (make-instance 'square) :colour 'red) (program-error () :bad-key))
(keyed (make-instance 'square) :colour 'red :allow-other-keys t)
(handler-case (keyed (make-instance 'square) :size) (program-error () :odd))

;; The instance protocol is generic.
(class-name (defclass rect () ((w :initarg :w :reader rect-w) (h :initarg :h :reader rect-h) (area :reader rect-area))))
(progn (defmethod initialize-instance :after ((r rect) &key) (setf (slot-value r 'area) (* (rect-w r) (rect-h r)))) :ok)
(rect-area (make-instance 'rect :w 3 :h 4))
(defvar *si-trace* nil)
(progn (defmethod shared-initialize :before ((r rect) slot-names &key) (push slot-names *si-trace*)) :ok)
(progn (make-instance 'rect :w 1 :h 1) (reinitialize-instance (make-instance 'rect :w 1 :h 1) :w 2) (reverse *si-trace*))
(class-name (defclass with-extra () ((v :reader v))))
(progn (defmethod initialize-instance :after ((x with-extra) &key extra) (setf (slot-value x 'v) (list :extra extra))) :ok)
(v (make-instance 'with-extra :extra 1))
(handler-case (make-instance 'with-extra :unknown 1) (program-error () :invalid))
(rect-w (make-instance (find-class 'rect) :w 9 :h 1))
(progn (defmethod slot-missing ((c t) (obj rect) name operation &optional value) (list :missing name operation)) :ok)
(slot-value (make-instance 'rect :w 1 :h 1) 'depth)
(class-name (defclass lazy () ((cache :reader cache))))
(progn (defmethod slot-unbound ((c t) (obj lazy) name) (list :computed name)) :ok)
(cache (make-instance 'lazy))
(handler-case (rect-w (make-instance 'lazy)) (error () :no-method))
(progn (defmethod rect-w :around ((r rect)) (list :around (call-next-method))) :ok)
(rect-w (make-instance 'rect :w 5 :h 1))
(list (typep #'rect-w 'generic-function) (typep #'make-instance 'generic-function) (typep #'initialize-instance 'generic-function))

;; A method with &rest and no &key does not widen the accepted keywords.
(progn (defgeneric f3 (x &key)) (defmethod f3 ((x integer) &rest r) r) (defmethod f3 ((x number) &key a) (list x a)) :ok)
(handler-case (f3 1 :b 2) (program-error () :bad-key))
(f3 1 :a 2)
;; An ordinary function does not become a generic function.
(defun ordf (x) (list :ordinary x))
(handler-case (defmethod ordf ((x integer)) x) (error () :refused))
(ordf 5)
(handler-case (defgeneric ordf (x)) (error () :refused))
;; An argument precedence order must name every required parameter once.
(handler-case (defgeneric ap2 (a b) (:argument-precedence-order b)) (error () :refused))
(handler-case (defgeneric ap3 (a b) (:argument-precedence-order b c)) (error () :refused))
;; Redefining a class removes the accessor methods its old definition made.
(class-name (defclass rr () ((a :initform 1 :reader rr-a))))
(rr-a (make-instance 'rr))
(class-name (defclass rr () ((a :initform 1))))
(handler-case (rr-a (make-instance 'rr)) (error () :no-method))
;; Making instances stays cheap enough for a loop of three hundred in one form.
(class-name (defclass bench () ((a :initarg :a))))
(let ((made nil)) (dotimes (i 300) (setq made (make-instance 'bench :a i))) (slot-value made 'a))
