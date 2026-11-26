;;;; structures.lisp — `defstruct`, all of CLHS 8 (step Z, #73)
;;;;
;;;; Tier 0's `defstruct`, in the prelude, made a vector with the type's name
;;;; in slot 0: enough for a compiler, and wrong for Common Lisp, because a
;;;; structure is not a vector. This one makes a *structure*, a heap object
;;;; of its own type (`ObjectType::Instance`, #73 decision 1). Its slot 0
;;;; holds the structure's **layout**, the object that describes its type,
;;;; and its slots follow. Four primitives reach it: `%make-structure`,
;;;; `%structure-layout` (nil for anything that is not a structure, so it is
;;;; the predicate too), `%structure-ref` and `%set-structure-ref`, whose
;;;; indices count the slots from 0. `vectorp`, `aref` and the sequence
;;;; functions refuse a structure without being told to.
;;;;
;;;; The library's own structures (string streams, packages, the readtable,
;;;; `format`'s directives) are defined by later files, so they are
;;;; structures too.
;;;;
;;;; This file must not use `defstruct` itself: the cold build would expand
;;;; it with the prelude's and a rebuild with this one, and the two worlds
;;;; would differ.

;;; ------------------------------------------------------------------------
;;; Layouts
;;;
;;; A layout is a simple vector, exactly what
;;;
;;;   (defstruct (%layout (:type vector)) name slots include printer
;;;     constructor type named conc-name length constructors predicate
;;;     copier tags)
;;;
;;; would make, written out by hand because this is the file that makes
;;; `defstruct`. A slot description is a list (name index initform type
;;; read-only). For a structure, INDEX counts its slots; for a structure of
;;; `:type list` or `vector`, it is the element's position, and TAGS lists
;;; the positions that hold a name, as (index . name).
;;;
;;; A layout is found by name, under the property `%layout`. A redefinition
;;; updates the old layout in place, so that structures made before it keep
;;; their type: a warm rebuild (chapter 41) redefines every structure the
;;; library has while those structures are in use.

(defun %layout-name (layout) (aref layout 0))
(defun %layout-slots (layout) (aref layout 1))
(defun %layout-include (layout) (aref layout 2))
(defun %layout-printer (layout) (aref layout 3))
(defun %layout-constructor (layout) (aref layout 4))
(defun %layout-type (layout) (aref layout 5))
(defun %layout-named (layout) (aref layout 6))
(defun %layout-conc-name (layout) (aref layout 7))
(defun %layout-length (layout) (aref layout 8))
(defun %layout-constructors (layout) (aref layout 9))
(defun %layout-predicate (layout) (aref layout 10))
(defun %layout-copier (layout) (aref layout 11))
(defun %layout-tags (layout) (aref layout 12))

;;; `aref` rather than `svref`: `aref` is a primitive and `svref` a library
;;; function with checks of its own, and every predicate that misses reads
;;; a layout (see `%structure-definitions`).

(defun %find-layout (name) (get name '%layout))

;;; Make NAME's layout say what DESCRIPTION says, and return it. PRINTER is
;;; the function from `:print-function` or `:print-object`, evaluated where
;;; the `defstruct` is, as (kind . function), or nil.
(defun %install-layout (description printer)
  (let ((layout (%find-layout (svref description 0))))
    (if (and layout (= (length layout) (length description)))
        (replace layout description)
        (progn (setq layout (copy-seq description))
               (setf (get (svref description 0) '%layout) layout)))
    (setf (aref layout 3) printer)
    layout))

;;; Is OBJECT a structure of LAYOUT's type, or of a type that includes it?
(defun %structure-is-a (object layout)
  (do ((l (%structure-layout object) (%layout-include l)))
      ((or (null l) (eq l layout)) (if l t nil))))

;;; OBJECT, when it has LAYOUT's type; a `type-error` otherwise. An accessor
;;; calls this only when OBJECT's layout is not LAYOUT itself.
(defun %check-structure (object layout)
  (if (%structure-is-a object layout)
      object
      (error 'type-error
             (%string-append "a structure of type " (symbol-name (%layout-name layout)))
             object)))

(defun copy-structure (structure)
  (let ((layout (%structure-layout structure)))
    (if (null layout) (error 'type-error "copy-structure wants a structure" structure))
    (let* ((n (%layout-length layout))
           (copy (%make-structure layout n)))
      (dotimes (i n copy)
        (%set-structure-ref copy i (%structure-ref structure i))))))

;;; `equalp` descends into two structures of the same type (CLHS `equalp`).
(defun %structure-equalp (a b)
  (let ((layout (%structure-layout a)))
    (and (eq layout (%structure-layout b))
         (dotimes (i (%layout-length layout) t)
           (if (not (equalp (%structure-ref a i) (%structure-ref b i)))
               (return nil))))))

;;; ------------------------------------------------------------------------
;;; Reading a `defstruct`

;;; A slot: `name`, `(name)`, `(name initform)`, or `(name initform :type
;;; type :read-only flag)`. The type is recorded and not checked; types are
;;; phase 3's.
(defun %struct-slot (spec index)
  (cond ((symbolp spec) (list spec index nil t nil))
        ((and (consp spec) (symbolp (car spec)))
         (destructuring-bind (name &optional initform &key (type t) read-only) spec
           (list name index initform type read-only)))
        (t (error "a defstruct slot is a name or (name initform option...)" spec))))

;;; An `:include` option may change an inherited slot's initform, type and
;;; read-only flag; it may make a slot read-only but not writable again.
(defun %struct-override (slot overrides)
  (let ((override (find (car slot) overrides
                        :key (lambda (o) (if (consp o) (car o) o))
                        :test #'string=)))
    (cond ((null override) (copy-list slot))
          ((symbolp override) (list (first slot) (second slot) nil (fourth slot) (fifth slot)))
          (t (destructuring-bind (name &optional initform &key (type (fourth slot))
                                                              (read-only (fifth slot)))
                 override
               (list name (second slot) initform type read-only))))))

;;; `"POINT-"`, or the prefix `:conc-name` gave: a string, a symbol, a
;;; character, or nothing at all, which means no prefix.
(defun %struct-conc-name (arguments)
  (cond ((null arguments) "")
        ((null (car arguments)) "")
        (t (string (car arguments)))))

;;; Everything the `defstruct` form says, as a layout (see above).
(defun %parse-defstruct (name options specs)
  (let ((conc-name (%string-append (symbol-name name) "-"))
        (constructors nil) (constructor-seen nil) (no-constructor nil)
        (predicate :default) (copier :default)
        (include nil) (overrides nil)
        (type nil) (element-type t) (named nil) (offset 0))
    (dolist (option options)
      (let ((key (if (consp option) (car option) option))
            (arguments (if (consp option) (cdr option) nil)))
        (case key
          (:conc-name (setq conc-name (%struct-conc-name arguments)))
          (:constructor
           (setq constructor-seen t)
           (cond ((null arguments) (push (cons (%symbol-concat 'make- name) :keys) constructors))
                 ((null (car arguments)) (setq no-constructor t))
                 ((cdr arguments) (push (cons (car arguments) (cadr arguments)) constructors))
                 (t (push (cons (car arguments) :keys) constructors))))
          (:predicate (setq predicate (if arguments (car arguments) :default)))
          (:copier (setq copier (if arguments (car arguments) :default)))
          (:include (setq include (car arguments) overrides (cdr arguments)))
          ((:print-function :print-object))
          (:type (setq type (let ((spec (car arguments)))
                              (cond ((eq spec 'list) 'list)
                                    ((or (eq spec 'vector) (and (consp spec) (eq (car spec) 'vector))) 'vector)
                                    (t (error "a defstruct :type is list or vector" spec))))
                       element-type (let ((spec (car arguments)))
                                      (if (and (consp spec) (cdr spec)) (cadr spec) t))))
          (:named (setq named t))
          (:initial-offset (setq offset (car arguments)))
          (t (error "not a defstruct option" option)))))
    (if (and (null type) (or named (/= offset 0)))
        (error ":named and :initial-offset want :type list or vector" name))
    ;; An unnamed list or vector has nothing to recognise it by, and a name
    ;; is a symbol, which a vector of some other element type cannot hold.
    (if (and type (not named) predicate (not (eq predicate :default)))
        (error "only a :named structure of :type list or vector has a predicate" name))
    (if (and named (not (eq element-type t)))
        (error "a :named structure's vector must hold any object, to hold its name" name))
    (if (and no-constructor constructors)
        (error "(:constructor nil) combined with other :constructors" name))
    (if (null constructor-seen)
        (push (cons (%symbol-concat 'make- name) :keys) constructors))
    (if (eq predicate :default)
        (setq predicate (if (or (null type) named) (%symbol-concat name '-p) nil)))
    (if (eq copier :default) (setq copier (%symbol-concat 'copy- name)))
    (let* ((parent (if include
                       (or (%find-layout include)
                           (error "defstruct :include names no structure" include))
                       nil))
           (slots (if parent
                      (mapcar (lambda (slot) (%struct-override slot overrides)) (%layout-slots parent))
                      nil))
           (tags (if parent (copy-list (%layout-tags parent)) nil))
           (index (if parent (%layout-length parent) 0)))
      (if (and parent (not (eq (%layout-type parent) type)))
          (error "defstruct :include wants a structure of the same :type" include))
      (setq index (+ index offset))
      (if named (progn (push (cons index name) tags) (setq index (+ index 1))))
      (dolist (spec (if (stringp (car specs)) (cdr specs) specs))
        (setq slots (append slots (list (%struct-slot spec index))))
        (setq index (+ index 1)))
      (setq constructors (reverse constructors))
      (vector name slots parent nil (car (rassoc :keys constructors))
              type named conc-name index constructors predicate copier tags))))

;;; ------------------------------------------------------------------------
;;; Writing the definitions

(defmacro defstruct (name-and-options &rest slots)
  (let* ((name (if (consp name-and-options) (car name-and-options) name-and-options))
         (options (if (consp name-and-options) (cdr name-and-options) nil))
         (description (%parse-defstruct name options slots))
         (printer (%struct-printer-form options))
         (layout (gensym "LAYOUT")))
    ;; Installed now as well as when the expansion runs, so that a later
    ;; `defstruct` that includes this one can be expanded before this one
    ;; has run, as SBCL allows.
    (%install-layout description nil)
    `(progn
       (let ((,layout (%install-layout ',description ,printer)))
         ,@(if (%layout-type description)
               (%typed-definitions description)
               (%structure-definitions description layout))
         ,layout)
       (%note-structure-defined ',name)
       ',name)))

;;; A structure's type is known as soon as its `defstruct` is expanded (see
;;; above), but it is defined, and a class (`find-class`, #124), only once
;;; the expansion has run: `(unless (find-class 'x nil) (defstruct x ...))`
;;; must define X.
(defun %note-structure-defined (name)
  (setf (get name '%structure-defined) t))

(defun %struct-printer-form (options)
  (let ((option (find-if (lambda (o) (and (consp o) (member (car o) '(:print-function :print-object))
                                          (cdr o)))
                         options)))
    (if option
        `(cons ,(if (eq (car option) :print-object) :object :function)
               (function ,(cadr option)))
        nil)))

;;; The name of SLOT's accessor in DESCRIPTION.
(defun %struct-accessor (description slot)
  (%symbol-concat (make-symbol (%layout-conc-name description)) (first slot)))

;;; An inherited slot whose accessor would have the name its parent's
;;; already has needs no new one: that accessor accepts this type too.
(defun %struct-inherited-accessor-p (description slot)
  (let ((parent (%layout-include description)))
    (and parent
         (find (first slot) (%layout-slots parent) :key #'car :test #'string=)
         (eq (%struct-accessor description slot)
             (%symbol-concat (make-symbol (%layout-conc-name parent)) (first slot))))))

;;; A structure's constructors, predicate, copier and accessors. Each closes
;;; over LAYOUT, the variable the expansion binds to the layout.
(defun %structure-definitions (description layout)
  (let ((object (gensym "OBJECT")) (value (gensym "VALUE")) (definitions nil))
    (dolist (constructor (%layout-constructors description))
      (push (%struct-constructor description (car constructor) (cdr constructor)
                                 (lambda (values)
                                   (let ((new (gensym "NEW")))
                                     `(let ((,new (%make-structure ,layout ,(length values))))
                                        ,@(let ((i -1))
                                            (mapcar (lambda (v) `(%set-structure-ref ,new ,(setq i (+ i 1)) ,v))
                                                    values))
                                        ,new))))
            definitions))
    (if (%layout-predicate description)
        ;; The object's own layout first, the common case, so that asking
        ;; whether a string stream is the console stays cheap: every write
        ;; asks (#73).
        (push `(defun ,(%layout-predicate description) (,object)
                 (let ((own (%structure-layout ,object)))
                   (cond ((eq own ,layout) t)
                         ((null own) nil)
                         (t (%structure-is-a ,object ,layout)))))
              definitions))
    (if (%layout-copier description)
        (push `(defun ,(%layout-copier description) (,object)
                 (copy-structure (%check-structure ,object ,layout)))
              definitions))
    (dolist (slot (%layout-slots description))
      (if (not (%struct-inherited-accessor-p description slot))
          (let ((reader (%struct-accessor description slot))
                (checked `(if (eq (%structure-layout ,object) ,layout)
                              ,object
                              (%check-structure ,object ,layout))))
            (push `(defun ,reader (,object) (%structure-ref ,checked ,(second slot))) definitions)
            (if (not (fifth slot))
                (let ((writer (%symbol-concat '%set- reader)))
                  (push `(defun ,writer (,object ,value)
                           (%set-structure-ref ,checked ,(second slot) ,value))
                        definitions)
                  (push `(%define-setf-writer ',reader ',writer) definitions))))))
    (nreverse definitions)))

;;; A structure of `:type list` or `vector` is a list or vector: the
;;; constructors build one, the accessors are `nth` and `aref`, and only a
;;; `:named` one can have a predicate, which looks for its name.
(defun %typed-definitions (description)
  (let ((object (gensym "OBJECT")) (value (gensym "VALUE")) (definitions nil)
        (listp (eq (%layout-type description) 'list)))
    (dolist (constructor (%layout-constructors description))
      (push (%struct-constructor description (car constructor) (cdr constructor)
                                 (lambda (values)
                                   (cons (if listp 'list 'vector) (%typed-elements description values))))
            definitions))
    (if (%layout-predicate description)
        (let ((index (car (rassoc (%layout-name description) (%layout-tags description)))))
          (push `(defun ,(%layout-predicate description) (,object)
                   ,(if listp
                        `(and (listp ,object) (eq (nth ,index ,object) ',(%layout-name description)))
                        `(and (vectorp ,object) (> (length ,object) ,index)
                              (eq (aref ,object ,index) ',(%layout-name description)))))
                definitions)))
    (if (%layout-copier description)
        (push `(defun ,(%layout-copier description) (,object)
                 (,(if listp 'copy-list 'copy-seq) ,object))
              definitions))
    (dolist (slot (%layout-slots description))
      (if (not (%struct-inherited-accessor-p description slot))
          (let ((reader (%struct-accessor description slot))
                (place (if listp `(nth ,(second slot) ,object) `(aref ,object ,(second slot)))))
            (push `(defun ,reader (,object) ,place) definitions)
            (if (not (fifth slot))
                (let ((writer (%symbol-concat '%set- reader)))
                  (push `(defun ,writer (,object ,value) (setf ,place ,value)) definitions)
                  (push `(%define-setf-writer ',reader ',writer) definitions))))))
    (nreverse definitions)))

;;; The element forms of a typed structure, position by position: a slot's
;;; value, a name where `:named` put one, nil where `:initial-offset` left a
;;; gap.
(defun %typed-elements (description values)
  (let ((elements nil))
    (dotimes (i (%layout-length description) (nreverse elements))
      (let ((slot (find i (%layout-slots description) :key #'second))
            (tag (assoc i (%layout-tags description))))
        (push (cond (slot (nth (position slot (%layout-slots description)) values))
                    (tag `',(cdr tag))
                    (t nil))
              elements)))))

;;; A constructor: with `:keys`, one keyword argument per slot, defaulting to
;;; its initform; otherwise a BOA ("by order of arguments") lambda list
;;; (CLHS 3.4.6). BUILD turns the slots' value forms, in slot order, into the
;;; form that makes the object.
(defun %struct-constructor (description name lambda-list build)
  (let ((slots (%layout-slots description)))
    (if (eq lambda-list :keys)
        `(defun ,name (&key ,@(mapcar (lambda (slot) (list (first slot) (third slot))) slots))
           ,(funcall build (mapcar #'first slots)))
        (let* ((boa (%boa-lambda-list lambda-list slots))
               (bound (cdr boa)))
          `(defun ,name ,(car boa)
             ,(funcall build
                       (mapcar (lambda (slot)
                                 (let ((variable (find (first slot) bound :test #'string=)))
                                   (if variable variable (third slot))))
                               slots)))))))

;;; A BOA lambda list as the constructor's own: an optional or keyword
;;; parameter that names a slot and gives no default takes the slot's
;;; initform; an `&aux` variable with no value leaves its slot nil. Returns
;;; (lambda-list . the variables it binds).
(defun %boa-lambda-list (lambda-list slots)
  (let ((state :required) (result nil) (bound nil))
    (dolist (item lambda-list)
      (if (member item '(&optional &rest &key &allow-other-keys &aux))
          (progn (setq state item) (push item result))
          (let* ((variable (cond ((symbolp item) item)
                                 ((and (eq state '&key) (consp (car item))) (cadr (car item)))
                                 (t (car item))))
                 (slot (find variable slots :key #'car :test #'string=))
                 (default-missing (or (symbolp item) (null (cdr item)))))
            (push variable bound)
            (push (if (and default-missing slot (member state '(&optional &key)))
                      (list (if (symbolp item) item (car item)) (third slot))
                      item)
                  result))))
    (cons (nreverse result) bound)))
