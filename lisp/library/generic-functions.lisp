;;;; generic-functions.lisp — generic functions and methods (phase 3, step
;;;; A3, #124)
;;;;
;;;; A generic function is a function whose code is chosen, each time it is
;;;; called, by the classes of its arguments:
;;;;
;;;;   (defgeneric area (shape))
;;;;   (defmethod area ((s circle)) (* pi (r s) (r s)))
;;;;   (defmethod area ((s square)) (* (side s) (side s)))
;;;;
;;;; Each `defmethod` adds a *method*, a piece of code with a *specializer*
;;;; for each required argument (a class, or `(eql object)`). A call finds
;;;; the methods whose specializers its arguments satisfy, orders them from
;;;; most to least specific (CLHS 7.6.6), and combines them into one
;;;; *effective method*: the most specific primary method runs and may pass
;;;; control to the next with `call-next-method`, around it run any
;;;; `:around` methods, and before and after it the `:before` and `:after`
;;;; methods (*standard method combination*, CLHS 7.6.6.2).
;;;;
;;;; A generic function is a funcallable instance (`clos.lisp`) whose
;;;; function is its *discriminating function*: a closure that takes the
;;;; classes of the required arguments, looks them up in the generic
;;;; function's cache, and on a miss computes the effective method and keeps
;;;; it there. Working out the applicable methods and their order is the
;;;; slow part, and a cache keyed on the argument classes means it is done
;;;; once per combination of classes (AMOP §1.8; PCL's caches do the same).
;;;; The effective method is built from closures. Compiling it would make it
;;;; faster to run, but would call the compiler from the middle of a call.
;;;;
;;;; Generic function and method objects keep their slots at fixed places,
;;;; read by the `%gf-…` and `%method-…` functions below, as classes do, so
;;;; that the dispatch machinery never itself dispatches.

;;; ------------------------------------------------------------------------
;;; The metaobject classes

(defclass generic-function (metaobject funcallable-standard-object) ()
  (:metaclass funcallable-standard-class))

(defclass standard-generic-function (generic-function)
  (name lambda-list methods method-combination argument-precedence-order cache epoch
   declarations documentation method-class initial-methods eql-positions plist)
  (:metaclass funcallable-standard-class))

(defclass method (metaobject) ())

(defclass standard-method (method)
  (generic-function specializers qualifiers lambda-list function documentation plist))

(defclass standard-accessor-method (standard-method) (slot-name))
(defclass standard-reader-method (standard-accessor-method) ())
(defclass standard-writer-method (standard-accessor-method) ())

(defclass eql-specializer (specializer) (object))

(defclass method-combination (metaobject) ())
(defclass standard-method-combination (method-combination) (name options))

;;; The places the readers below index must be the places the classes give
;;; their slots; this says so when the file is loaded, rather than as wrong
;;; answers later.
(defun %check-slot-places (class-name names)
  (unless (equalp (%wrapper-slot-names (%class-wrapper (find-class class-name)))
                  (coerce names 'simple-vector))
    (error "a metaobject class's slots are not where its readers look" class-name)))

(%check-slot-places 'standard-generic-function
                    '(name lambda-list methods method-combination argument-precedence-order
                      cache epoch declarations documentation method-class initial-methods
                      eql-positions plist))
(%check-slot-places 'standard-method
                    '(generic-function specializers qualifiers lambda-list function
                      documentation plist))
(%check-slot-places 'standard-reader-method
                    '(generic-function specializers qualifiers lambda-list function
                      documentation plist slot-name))

(defun %slot-at (object index) (aref (%instance-slots object) index))
(defun %set-slot-at (object index value) (setf (aref (%instance-slots object) index) value))

(defun %gf-name (gf) (%slot-at gf 0))
(defun %gf-lambda-list (gf) (%slot-at gf 1))
(defun %gf-methods (gf) (%slot-at gf 2))
(defun %gf-method-combination (gf) (%slot-at gf 3))
(defun %gf-argument-precedence-order (gf) (%slot-at gf 4))
(defun %gf-cache (gf) (%slot-at gf 5))
(defun %gf-epoch (gf) (%slot-at gf 6))
(defun %gf-declarations (gf) (%slot-at gf 7))
(defun %gf-documentation (gf) (%slot-at gf 8))
(defun %gf-method-class (gf) (%slot-at gf 9))
(defun %gf-initial-methods (gf) (%slot-at gf 10))
(defun %gf-eql-positions (gf) (%slot-at gf 11))

(defun %set-gf-name (gf value) (%set-slot-at gf 0 value))
(defun %set-gf-lambda-list (gf value) (%set-slot-at gf 1 value))
(defun %set-gf-methods (gf value) (%set-slot-at gf 2 value))
(defun %set-gf-method-combination (gf value) (%set-slot-at gf 3 value))
(defun %set-gf-argument-precedence-order (gf value) (%set-slot-at gf 4 value))
(defun %set-gf-cache (gf value) (%set-slot-at gf 5 value))
(defun %set-gf-epoch (gf value) (%set-slot-at gf 6 value))
(defun %set-gf-declarations (gf value) (%set-slot-at gf 7 value))
(defun %set-gf-documentation (gf value) (%set-slot-at gf 8 value))
(defun %set-gf-method-class (gf value) (%set-slot-at gf 9 value))
(defun %set-gf-initial-methods (gf value) (%set-slot-at gf 10 value))
(defun %set-gf-eql-positions (gf value) (%set-slot-at gf 11 value))

(defun %method-generic-function (method) (%slot-at method 0))
(defun %method-specializers (method) (%slot-at method 1))
(defun %method-qualifiers (method) (%slot-at method 2))
(defun %method-lambda-list (method) (%slot-at method 3))
(defun %method-function (method) (%slot-at method 4))
(defun %method-documentation (method) (%slot-at method 5))
(defun %method-slot-name (method) (%slot-at method 7))
(defun %set-method-generic-function (method value) (%set-slot-at method 0 value))

(defun %eql-specializer-object (specializer) (%slot-at specializer 0))

(defun %eql-specializer-p (object)
  (and (%instance-wrapper object) (eq (class-of object) (find-class 'eql-specializer))))

;;; ------------------------------------------------------------------------
;;; eql specializers

;;; One specializer per object, so that two methods on `(eql 7)` have the
;;; same specializer and `find-method` can compare them with `eq`.
(defvar %eql-specializers (make-hash-table :test 'eql))

(defun intern-eql-specializer (object)
  (or (gethash object %eql-specializers)
      (let ((specializer (%allocate-instance (find-class 'eql-specializer))))
        (%set-slot-at specializer 0 object)
        (setf (gethash object %eql-specializers) specializer))))

;;; ------------------------------------------------------------------------
;;; Lambda lists

;;; The lambda list keywords. `lambda-list-keywords` is defined by
;;; `eval.lisp`, which loads after this file, and the methods below are
;;; defined while the library loads.
(defvar %lambda-markers '(&optional &rest &body &key &allow-other-keys &aux &whole &environment))

;;; The number of required parameters in LAMBDA-LIST.
(defun %required-count (lambda-list)
  (let ((count 0))
    (dolist (item lambda-list count)
      (if (member item %lambda-markers) (return count) (setq count (+ count 1))))))

;;; The parts of a lambda list that congruence compares (CLHS 7.6.4): how
;;; many required and optional parameters, whether it has `&rest`, `&key`
;;; or `&allow-other-keys`, and its keyword names.
(defun %lambda-list-shape (lambda-list)
  (let ((required 0) (optional 0) (rest nil) (key nil) (allow nil) (keywords nil) (state :required))
    (dolist (item lambda-list)
      (cond ((eq item '&optional) (setq state :optional))
            ((member item '(&rest &body)) (setq rest t state :rest))
            ((eq item '&key) (setq key t state :key))
            ((eq item '&allow-other-keys) (setq allow t))
            ((eq item '&aux) (setq state :aux))
            ((member item %lambda-markers))
            ((eq state :required) (setq required (+ required 1)))
            ((eq state :optional) (setq optional (+ optional 1)))
            ((eq state :key) (push (%keyword-of item) keywords))))
    (list required optional rest key allow (nreverse keywords))))

;;; The keyword a `&key` parameter takes: `x`, `(x 1)`, `((:k x) 1)`.
(defun %keyword-of (parameter)
  (cond ((symbolp parameter) (intern (symbol-name parameter) "KEYWORD"))
        ((consp (car parameter)) (car (car parameter)))
        (t (intern (symbol-name (car parameter)) "KEYWORD"))))

;;; The lambda list a generic function gets when `defmethod` makes it
;;; (CLHS 7.6.4): the method's, without specializers, default forms,
;;; keyword parameters or `&aux`.
(defun %generic-lambda-list (lambda-list)
  (let ((result nil) (state :required))
    (dolist (item lambda-list (nreverse result))
      (cond ((eq item '&aux) (return (nreverse result)))
            ((member item '(&optional &rest &body))
             (setq state item) (push (if (eq item '&body) '&rest item) result))
            ((eq item '&key) (setq state :key) (push '&key result))
            ((eq item '&allow-other-keys) nil)
            ((eq state :key) nil)
            ((consp item) (push (car item) result))
            (t (push item result))))))

;;; Are a method's lambda list and its generic function's congruent (CLHS
;;; 7.6.4)? The same numbers of required and optional parameters; if
;;; either has `&rest` or `&key`, both do; and a method accepts every
;;; keyword the generic function names.
(defun %congruent-p (gf-lambda-list method-lambda-list)
  (let ((g (%lambda-list-shape gf-lambda-list))
        (m (%lambda-list-shape method-lambda-list)))
    (and (= (first g) (first m))
         (= (second g) (second m))
         (eq (or (third g) (fourth g)) (or (third m) (fourth m)))
         (or (not (fourth g))
             (third m) (fifth m)
             (every (lambda (k) (member k (sixth m))) (sixth g))))))

;;; ------------------------------------------------------------------------
;;; Generic functions

;;; The function NAME names, or nil. `fdefinition` asks
;;; `special-operator-p`, which `eval.lisp` defines after this file, and
;;; generic functions are made while the library loads.
(defun %defined-function (name)
  (let ((cell (%function-cell name)))
    (and cell (%fboundp cell) (symbol-function cell))))

(defun %make-generic-function (name lambda-list)
  (let ((gf (%allocate-instance (find-class 'standard-generic-function))))
    ;; Every slot starts as nil: `%allocate-instance` leaves them unbound,
    ;; and the readers here index them directly.
    (dotimes (i (length (%instance-slots gf))) (%set-slot-at gf i nil))
    (%set-gf-name gf name)
    (%set-gf-lambda-list gf lambda-list)
    (%set-gf-methods gf nil)
    (%set-gf-method-combination gf 'standard)
    (%set-gf-epoch gf nil)
    (%set-gf-method-class gf (find-class 'standard-method))
    (%install-discriminator gf)
    gf))

(defun %generic-function-p (object)
  (and (%funcallable-instance-p object)
       (member (find-class 'generic-function) (%class-precedence-list (class-of object)))
       t))

;;; Every call of GF comes here, by the closure that is its function.
(defun %install-discriminator (gf)
  (%set-funcallable-instance-function gf (lambda (&rest args) (%dispatch gf args)))
  gf)

;;; The generic function NAME names, made if there is none. An ordinary
;;; function, a macro or a special operator there is an error (CLHS
;;; `ensure-generic-function`, and SBCL): replacing an ordinary function
;;; would let `(defmethod reverse ...)` take `reverse` from the reader.
(defun ensure-generic-function (name &key (lambda-list nil lambda-list-p) documentation
                                          (method-combination nil) argument-precedence-order
                                          declare &allow-other-keys)
  (let ((existing (%defined-function name)))
    (when (and (symbolp name) (or (macro-function name) (%special-operator-p name)))
      (error 'program-error "a macro or special operator cannot become a generic function" name))
    (when (and existing (not (%generic-function-p existing)))
      (error 'program-error "an ordinary function cannot become a generic function" name))
    (let ((gf (if (%generic-function-p existing)
                  existing
                  (let ((new (%make-generic-function name lambda-list)))
                    (setf (fdefinition name) new)
                    new))))
      (when lambda-list-p
        (dolist (method (%gf-methods gf))
          (unless (%congruent-p lambda-list (%method-lambda-list method))
            (error "a method of this generic function does not fit the new lambda list" name)))
        (%set-gf-lambda-list gf lambda-list))
      (when (and method-combination (not (member method-combination '(standard (standard)) :test #'equal)))
        (error "only standard method combination exists yet (CLOS sub-step 6)" method-combination))
      (when argument-precedence-order
        (%set-gf-argument-precedence-order
         gf (%precedence-positions (%gf-lambda-list gf) argument-precedence-order)))
      (when documentation (%set-gf-documentation gf documentation))
      (when declare (%set-gf-declarations gf declare))
      (%invalidate-dispatch)
      gf)))

;;; ------------------------------------------------------------------------
;;; defgeneric

;;; `(defgeneric name lambda-list option*)`. A `defgeneric` of a name that
;;; is already a generic function keeps the object, removes the methods the
;;; previous `defgeneric` defined with `:method` options, and defines the
;;; ones this one names (CLHS `defgeneric`).
(defmacro defgeneric (name lambda-list &rest options)
  (let ((methods nil) (keys nil))
    (dolist (option options)
      (case (car option)
        (:method (push `(defmethod ,name ,@(cdr option)) methods))
        (:documentation (setq keys (list* :documentation (second option) keys)))
        (:argument-precedence-order
         (setq keys (list* :argument-precedence-order `',(cdr option) keys)))
        (:method-combination (setq keys (list* :method-combination `',(cdr option) keys)))
        ((:generic-function-class :method-class)
         (unless (member (second option) '(standard-generic-function standard-method))
           (error "only the standard generic function and method classes exist yet"
                  (second option))))
        (declare (setq keys (list* :declare `',(cdr option) keys)))
        (t (error 'program-error "an unknown defgeneric option" (car option)))))
    (let ((gf (gensym "GF")))
      `(let ((,gf (%defgeneric ',name ',lambda-list ,@keys)))
         (%set-gf-initial-methods ,gf (list ,@(nreverse methods)))
         ,gf))))

(defun %defgeneric (name lambda-list &rest keys)
  (when (getf keys :argument-precedence-order)
    (%precedence-positions lambda-list (getf keys :argument-precedence-order)))
  (let ((existing (%defined-function name)))
    (when (%generic-function-p existing)
      (dolist (method (%gf-initial-methods existing)) (remove-method existing method))
      (%set-gf-initial-methods existing nil))
    (apply #'ensure-generic-function name :lambda-list lambda-list keys)))

;;; The positions of the parameters ORDER names among LAMBDA-LIST's required
;;; ones; ORDER must name every required parameter exactly once.
(defun %precedence-positions (lambda-list order)
  (let ((required (let ((found nil))
                    (dolist (item lambda-list (nreverse found))
                      (if (member item %lambda-markers) (return (nreverse found)))
                      (push item found)))))
    (unless (and (= (length order) (length required))
                 (every (lambda (name) (member name required)) order)
                 (= (length (remove-duplicates order)) (length order)))
      (error 'program-error "the argument precedence order must name each required parameter once"
             order))
    (mapcar (lambda (name) (position name required)) order)))

;;; ------------------------------------------------------------------------
;;; Methods

(defun %make-method (class qualifiers specializers lambda-list function documentation)
  (let ((method (%allocate-instance class)))
    (%set-slot-at method 0 nil)
    (%set-slot-at method 1 specializers)
    (%set-slot-at method 2 qualifiers)
    (%set-slot-at method 3 lambda-list)
    (%set-slot-at method 4 function)
    (%set-slot-at method 5 documentation)
    method))

;;; The positions in which some method of GF has an `eql` specializer.
(defun %compute-eql-positions (gf)
  (let ((positions nil))
    (dolist (method (%gf-methods gf) positions)
      (let ((i 0))
        (dolist (specializer (%method-specializers method))
          (when (and (%eql-specializer-p specializer) (not (member i positions)))
            (push i positions))
          (setq i (+ i 1)))))))

(defun %same-specializers-p (a b)
  (and (= (length a) (length b)) (every #'eq a b)))

;;; The printer sends a cons, a symbol, a number, a string and the other
;;; built-in objects to `print-object` only once a program has defined a
;;; `print-object` method that could apply to one (CLHS 22.1.2 allows it;
;;; 11.1.2.1.2 leaves it undefined for standard classes, and Quokka honours
;;; it everywhere). Until then the printer goes straight to its own code,
;;; which keeps printing data as cheap as it was before CLOS (#124). Set by
;;; `add-method`, never cleared: with it set, printing is only slower.
(defvar %print-object-on-built-in nil)

(defun %note-print-object-method (gf method)
  (when (eq (%gf-name gf) 'print-object)
    (let* ((specializer (car (%method-specializers method)))
           (class (if (%eql-specializer-p specializer)
                      (class-of (%eql-specializer-object specializer))
                      specializer)))
      (when (and (not (eq class (find-class t)))
                 (eq (class-of class) (find-class 'built-in-class)))
        (setq %print-object-on-built-in t)))))

(defun add-method (gf method)
  (when (and (%method-generic-function method) (not (eq (%method-generic-function method) gf)))
    (error "this method already belongs to another generic function" method))
  (unless (%congruent-p (%gf-lambda-list gf) (%method-lambda-list method))
    (error "the method's lambda list does not fit its generic function's" (%gf-name gf)))
  (let ((old (find-if (lambda (m)
                        (and (%same-specializers-p (%method-specializers m) (%method-specializers method))
                             (equal (%method-qualifiers m) (%method-qualifiers method))))
                      (%gf-methods gf))))
    (when old
      (%set-method-generic-function old nil)
      (%set-gf-methods gf (remove old (%gf-methods gf)))))
  (%set-method-generic-function method gf)
  (%set-gf-methods gf (cons method (%gf-methods gf)))
  (%set-gf-eql-positions gf (%compute-eql-positions gf))
  (%note-print-object-method gf method)
  (%invalidate-dispatch)
  gf)

(defun remove-method (gf method)
  (when (member method (%gf-methods gf))
    (%set-gf-methods gf (remove method (%gf-methods gf)))
    (%set-method-generic-function method nil)
    (%set-gf-eql-positions gf (%compute-eql-positions gf))
    (%invalidate-dispatch))
  gf)

;;; A specializer as a program names it to `find-method`: a class, a class
;;; name, or `(eql object)`.
(defun %specializer-designated (designator)
  (cond ((symbolp designator) (find-class designator))
        ((and (consp designator) (eq (car designator) 'eql))
         (intern-eql-specializer (second designator)))
        (t designator)))

(defun find-method (gf qualifiers specializers &optional (errorp t))
  (let ((specializers (mapcar #'%specializer-designated specializers)))
    (unless (= (length specializers) (%required-count (%gf-lambda-list gf)))
      (error "find-method was given the wrong number of specializers" specializers))
    (or (find-if (lambda (m)
                   (and (%same-specializers-p (%method-specializers m) specializers)
                        (equal (%method-qualifiers m) qualifiers)))
                 (%gf-methods gf))
        (if errorp
            (error "there is no method with these qualifiers and specializers" (%gf-name gf))
            nil))))

(defun method-qualifiers (method) (%method-qualifiers method))

;;; `(%ensure-method name qualifiers specializers lambda-list function
;;; documentation)`: what `defmethod` does when it runs.
(defun %ensure-method (name qualifiers specializers lambda-list function documentation)
  (let* ((gf (if (%generic-function-p (%defined-function name))
                 (%defined-function name)
                 (ensure-generic-function name :lambda-list (%generic-lambda-list lambda-list))))
         (method (%make-method (%gf-method-class gf) qualifiers specializers
                               (%plain-lambda-list lambda-list) function documentation)))
    (add-method gf method)
    method))

;;; LAMBDA-LIST without its specializers.
(defun %plain-lambda-list (lambda-list)
  (let ((result nil) (required t))
    (dolist (item lambda-list (nreverse result))
      (when (member item %lambda-markers) (setq required nil))
      (push (if (and required (consp item)) (car item) item) result))))

;;; ------------------------------------------------------------------------
;;; defmethod

;;; Does FORM mention any of SYMBOLS anywhere? `defmethod` makes the local
;;; functions `call-next-method` and `next-method-p` only for a body that
;;; uses them.
(defun %mentions-p (form symbols)
  (cond ((symbolp form) (if (member form symbols) t nil))
        ((consp form) (or (%mentions-p (car form) symbols) (%mentions-p (cdr form) symbols)))
        (t nil)))

;;; A specializer as `defmethod` writes it, as the form that makes it when
;;; the `defmethod` runs.
(defun %specializer-form (specializer)
  (cond ((and (consp specializer) (eq (car specializer) 'eql))
         `(intern-eql-specializer ,(second specializer)))
        ((symbolp specializer) `(find-class ',specializer))
        (t (error 'program-error "this is not a specializer" specializer))))

(defmacro defmethod (name &rest rest)
  (let ((qualifiers nil))
    (do () ((listp (car rest))) (push (pop rest) qualifiers))
    (let* ((qualifiers (nreverse qualifiers))
           (lambda-list (pop rest))
           (documentation (if (and (stringp (car rest)) (cdr rest)) (pop rest) nil))
           (declarations (%body-declarations rest))
           (forms (%body-forms rest))
           (required (let ((found nil))
                       (dolist (item lambda-list (nreverse found))
                         (if (member item %lambda-markers) (return (nreverse found)))
                         (push item found))))
           (specializers (mapcar (lambda (p) (if (consp p) (second p) t)) required))
           (plain (%plain-lambda-list lambda-list))
           ;; A method with `&key` accepts keywords other methods accept
           ;; (CLHS 7.6.5); the generic function checks them as a whole.
           (inner (if (and (member '&key plain) (not (member '&allow-other-keys plain)))
                      (%insert-allow-other-keys plain)
                      plain))
           (block-name (if (consp name) (second name) name))
           (args (gensym "ARGS")) (next (gensym "NEXT")) (method (gensym "METHOD"))
           (call `(apply (lambda ,inner ,@declarations (block ,block-name ,@forms)) ,args)))
      `(let ((,method nil))
         (setq ,method
               (%ensure-method
                ',name ',qualifiers (list ,@(mapcar #'%specializer-form specializers))
                ',lambda-list
                (lambda (,args ,next)
                  ,(if (%mentions-p forms '(call-next-method next-method-p))
                       `(flet ((call-next-method (&rest more)
                                 (let ((arguments (or more ,args)))
                                   (if ,next
                                       (funcall ,next arguments)
                                       (apply #'no-next-method
                                              (%method-generic-function ,method) ,method arguments))))
                               (next-method-p () (if ,next t nil)))
                          ,call)
                       call))
                ,documentation))))))

;;; LAMBDA-LIST with `&allow-other-keys` after its keyword parameters.
(defun %insert-allow-other-keys (lambda-list)
  (let ((result nil) (inserted nil))
    (dolist (item lambda-list)
      (when (and (eq item '&aux) (not inserted))
        (push '&allow-other-keys result)
        (setq inserted t))
      (push item result))
    (unless inserted (push '&allow-other-keys result))
    (nreverse result)))

;;; ------------------------------------------------------------------------
;;; Dispatch

;;; The methods of GF that apply to ARGS, most specific first (CLHS 7.6.6).
(defun %compute-applicable-methods (gf args)
  (let ((applicable (remove-if-not (lambda (m) (%method-applicable-p m args)) (%gf-methods gf)))
        (order (or (%gf-argument-precedence-order gf)
                   (let ((positions nil))
                     (dotimes (i (%required-count (%gf-lambda-list gf)) (nreverse positions))
                       (push i positions))))))
    (sort applicable (lambda (a b) (%more-specific-p a b args order)))))

(defun compute-applicable-methods (gf arguments)
  (%compute-applicable-methods gf arguments))

(defun %method-applicable-p (method args)
  (do ((specializers (%method-specializers method) (cdr specializers))
       (rest args (cdr rest)))
      ((null specializers) t)
    (unless (%specializer-accepts-p (car specializers) (car rest))
      (return nil))))

(defun %specializer-accepts-p (specializer object)
  (if (%eql-specializer-p specializer)
      (eql (%eql-specializer-object specializer) object)
      (if (member specializer (%class-precedence-list (class-of object))) t nil)))

;;; Is method A more specific than method B for ARGS? Their specializers
;;; are compared one argument at a time, in ORDER: the first that differ
;;; decides, an `eql` specializer before a class, and of two classes the
;;; one that comes first in the argument's class precedence list.
(defun %more-specific-p (a b args order)
  (dolist (i order nil)
    (let ((sa (nth i (%method-specializers a)))
          (sb (nth i (%method-specializers b))))
      (unless (eq sa sb)
        (return (cond ((%eql-specializer-p sa) t)
                      ((%eql-specializer-p sb) nil)
                      (t (let ((cpl (%class-precedence-list (class-of (nth i args)))))
                           (< (position sa cpl) (position sb cpl))))))))))

;;; The key a call's effective method is cached under: the class of each
;;; required argument, or, where some method has an `eql` specializer and
;;; the argument has one, that specializer.
(defun %dispatch-key (gf args count)
  (let ((key nil) (positions (%gf-eql-positions gf)))
    (do ((i 0 (+ i 1)) (rest args (cdr rest)))
        ((= i count) (nreverse key))
      (push (or (and positions (member i positions) (gethash (car rest) %eql-specializers))
                (class-of (car rest)))
            key))))

(defun %dispatch (gf args)
  (let ((count (%required-count (%gf-lambda-list gf))))
    (unless (do ((i 0 (+ i 1)) (rest args (cdr rest)))
                ((= i count) t)
              (unless (consp rest) (return nil)))
      (error 'program-error "too few arguments to a generic function" (%gf-name gf)))
    (unless (eq (%gf-epoch gf) %dispatch-epoch)
      (%set-gf-cache gf nil)
      (%set-gf-epoch gf %dispatch-epoch))
    (let* ((key (%dispatch-key gf args count))
           (cached (assoc key (%gf-cache gf) :test #'%same-specializers-p)))
      (funcall (if cached
                   (cdr cached)
                   (let ((emf (%compute-effective-method gf args)))
                     (%set-gf-cache gf (cons (cons key emf) (%gf-cache gf)))
                     emf))
               args))))

;;; ------------------------------------------------------------------------
;;; Standard method combination (CLHS 7.6.6.2)

;;; An effective method is a function of the argument list. A method
;;; function is a function of the argument list and the effective method of
;;; the methods after it (nil when there are none), which `call-next-method`
;;; calls.
(defun %compute-effective-method (gf args)
  (let ((methods (%compute-applicable-methods gf args)))
    (if (null methods)
        (lambda (args) (apply #'no-applicable-method gf args))
        (%standard-effective-method gf methods))))

(defun %standard-effective-method (gf methods)
  (let ((around nil) (before nil) (primary nil) (after nil))
    (dolist (method methods)
      (let ((qualifiers (%method-qualifiers method)))
        (cond ((null qualifiers) (push method primary))
              ((equal qualifiers '(:around)) (push method around))
              ((equal qualifiers '(:before)) (push method before))
              ((equal qualifiers '(:after)) (push method after))
              (t (error "standard method combination does not know these qualifiers"
                        qualifiers)))))
    (setq around (nreverse around) before (nreverse before) primary (nreverse primary))
    ;; AFTER is now least specific first, the order the after methods run.
    (if (null primary)
        (lambda (args) args (error "no primary method applies" (%gf-name gf)))
        (let* ((primary-chain (%method-chain primary nil))
               (core (if (or before after)
                         (lambda (args)
                           (dolist (m before) (funcall (%method-function m) args nil))
                           (multiple-value-prog1 (funcall primary-chain args)
                             (dolist (m after) (funcall (%method-function m) args nil))))
                         primary-chain))
               (effective (if around (%method-chain around core) core)))
          (%with-keyword-check gf methods effective)))))

;;; When GF takes keyword arguments, EFFECTIVE wrapped in the check CLHS
;;; 7.6.5 asks for: every keyword in a call must be accepted by the generic
;;; function or by one of the applicable METHODS, unless one of them allows
;;; other keys or the call says `:allow-other-keys t`. Each method itself
;;; was made to allow other keys (`defmethod`), so this is the only check.
(defun %with-keyword-check (gf methods effective)
  (let ((shape (%lambda-list-shape (%gf-lambda-list gf))))
    (if (not (fourth shape))
        effective
        (let ((positional (+ (first shape) (second shape)))
              (valid (sixth shape))
              (any (fifth shape)))
          (dolist (method methods)
            (let ((m (%lambda-list-shape (%method-lambda-list method))))
              ;; A method with `&rest` but no `&key` does not affect which
              ;; keywords are accepted (CLHS 7.6.5); only `&allow-other-keys` does.
              (when (fifth m) (setq any t))
              (setq valid (append (sixth m) valid))))
          (lambda (args)
            (let ((keys (nthcdr positional args)))
              (unless (evenp (length keys))
                (error 'program-error "an odd number of keyword arguments" (%gf-name gf)))
              (unless (or any (getf keys :allow-other-keys))
                (do ((rest keys (cddr rest)))
                    ((null rest))
                  (unless (or (member (car rest) valid) (eq (car rest) :allow-other-keys))
                    (error 'program-error "a keyword argument no applicable method accepts"
                           (car rest))))))
            (funcall effective args))))))

;;; METHODS as an effective method, each method's next being the rest, and
;;; the last one's LAST.
(defun %method-chain (methods last)
  (if (null methods)
      last
      (let ((function (%method-function (car methods)))
            (next (%method-chain (cdr methods) last)))
        (lambda (args) (funcall function args next)))))

;;; ------------------------------------------------------------------------
;;; When no method applies

(defmethod no-applicable-method ((gf t) &rest arguments)
  (error "no method of this generic function applies to these arguments"
         (list (%gf-name gf) arguments)))

(defmethod no-next-method ((gf t) (method t) &rest arguments)
  (error "call-next-method was called and there is no next method"
         (list (%gf-name gf) arguments)))

;;; ------------------------------------------------------------------------
;;; The instance protocol (CLHS 7.1), generic

(defgeneric allocate-instance (class &rest initargs &key &allow-other-keys))
(defmethod allocate-instance ((class standard-class) &rest initargs)
  initargs
  (%allocate-instance class))
(defmethod allocate-instance ((class funcallable-standard-class) &rest initargs)
  initargs
  (%allocate-instance class))

(defgeneric make-instance (class &rest initargs &key &allow-other-keys))
(defmethod make-instance ((class symbol) &rest initargs)
  (apply #'make-instance (find-class class) initargs))
(defmethod make-instance ((class standard-class) &rest initargs)
  (apply #'%make-instance class initargs))
(defmethod make-instance ((class funcallable-standard-class) &rest initargs)
  (apply #'%make-instance class initargs))

(defgeneric initialize-instance (instance &rest initargs &key &allow-other-keys))
(defmethod initialize-instance ((instance standard-object) &rest initargs)
  (apply #'shared-initialize instance t initargs)
  instance)

(defgeneric reinitialize-instance (instance &rest initargs &key &allow-other-keys))
(defmethod reinitialize-instance ((instance standard-object) &rest initargs)
  (apply #'%reinitialize-instance instance initargs))

(defgeneric shared-initialize (instance slot-names &rest initargs &key &allow-other-keys))
(defmethod shared-initialize ((instance standard-object) slot-names &rest initargs)
  (apply #'%shared-initialize instance slot-names initargs))

(defgeneric slot-missing (class object slot-name operation &optional new-value))
(defmethod slot-missing ((class t) object slot-name operation &optional new-value)
  operation new-value
  (%slot-missing object slot-name))

(defgeneric slot-unbound (class instance slot-name))
(defmethod slot-unbound ((class t) instance slot-name)
  (%slot-unbound instance slot-name))

;;; `%method-initargs`' answers, by class and list of functions, each with
;;; the epoch it was computed in: it depends only on those, and
;;; `make-instance` asks on every call.
(defvar %method-initargs-cache (make-hash-table :test 'eq))

(defun %method-initargs (class functions)
  (let* ((entries (gethash class %method-initargs-cache))
         (entry (assoc functions entries)))
    (if (and entry (eq (second entry) %dispatch-epoch))
        (values (third entry) (fourth entry))
        (multiple-value-bind (keys any) (%compute-method-initargs class functions)
          (setf (gethash class %method-initargs-cache)
                (cons (list functions %dispatch-epoch keys any)
                      (remove functions entries :key #'car)))
          (values keys any)))))

;;; The keywords the applicable methods of the generic functions named in
;;; FUNCTIONS accept, for an instance of CLASS (CLHS 7.1.2), and whether one
;;; of them accepts any. A method applies when its first specializer is in
;;; CLASS's precedence list (for `allocate-instance` and `make-instance`,
;;; whose first argument is the class itself, in its metaclass's).
(defun %compute-method-initargs (class functions)
  (let ((keys nil) (any nil))
    (dolist (name functions)
      (let ((gf (%defined-function name)))
        (when (%generic-function-p gf)
          (let ((cpl (%class-precedence-list
                      (if (member name '(allocate-instance make-instance)) (class-of class) class))))
            (dolist (method (%gf-methods gf))
              (let ((first (car (%method-specializers method))))
                (when (member first cpl)
                  (let ((shape (%lambda-list-shape (%method-lambda-list method))))
                    (when (fifth shape) (setq any t))
                    (setq keys (append (sixth shape) keys))))))))))
    (values keys any)))

;;; ------------------------------------------------------------------------
;;; Accessors

;;; The generic function NAME, made with LAMBDA-LIST if there is none.
(defun %accessor-generic-function (name lambda-list)
  (if (%generic-function-p (%defined-function name))
      (%defined-function name)
      (ensure-generic-function name :lambda-list lambda-list)))

(defun %add-reader-method (class name slot-name)
  (let ((method (%make-method (find-class 'standard-reader-method) nil (list class) '(object)
                              (lambda (args next) next (slot-value (car args) slot-name))
                              nil)))
    (%set-slot-at method 7 slot-name)
    (add-method (%accessor-generic-function name '(object)) method)
    method))

(defun %add-writer-method (class name slot-name)
  (let ((method (%make-method (find-class 'standard-writer-method) nil
                              (list (find-class t) class) '(new-value object)
                              (lambda (args next)
                                next
                                (setf (slot-value (second args) slot-name) (first args)))
                              nil)))
    (%set-slot-at method 7 slot-name)
    (add-method (%accessor-generic-function name '(new-value object)) method)
    method))
