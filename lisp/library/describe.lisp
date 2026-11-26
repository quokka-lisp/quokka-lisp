;;;; `describe` and `describe-object` (CLOS sub-step 4, #124)
;;;;
;;;; `describe` writes a description of an object for a person to read, by
;;;; calling the generic function `describe-object`, so a program can say
;;;; how its own objects are described (CLHS 25.1.4). The text itself is
;;;; left to each implementation; Quokka's follows SBCL's in outline: the
;;;; object as `prin1` writes it, its kind in brackets, then, after a blank
;;;; line, what that kind of object has — an instance's slots, a symbol's
;;;; meanings, a class's superclasses.
;;;;
;;;; Loaded after `format.lisp` and before `eval.lisp`. The methods run long
;;;; after both, so they may call anything; the file itself, as it loads,
;;;; calls only `defgeneric` and `defmethod`.

;;; `(describe object &optional stream)`: STREAM is an output stream
;;; designator, nil for `*standard-output*` and t for `*terminal-io*`.
;;; Nothing is written but what `describe-object` writes, and no value is
;;; returned, as in SBCL.
(defun describe (object &optional stream)
  (describe-object object (%output-stream stream))
  (values))

(defgeneric describe-object (object stream))

;;; The first two lines of every description: `5` and `  [fixnum]`.
(defun %describe-header (object stream)
  (prin1 object stream)
  (format stream "~%  [~(~A~)]~%" (symbol-name (%describe-kind object))))

;;; The kind in brackets: what SBCL writes there, a broad kind for an
;;; instance, a structure, a condition or a list, and otherwise the class.
(defun %describe-kind (object)
  (cond ((listp object) (if object 'list 'null))
        ((typep object 'standard-object) 'standard-object)
        ((typep object 'structure-object) 'structure-object)
        ((typep object 'condition) 'condition)
        (t (class-name (class-of object)))))

(defmethod describe-object ((object t) stream)
  (%describe-header object stream))

;;; One slot: `  X                              = 1`, the name padded to
;;; thirty characters as SBCL pads it.
(defun %describe-slot (name boundp value stream)
  (let ((text (princ-to-string name)))
    (write-string "  " stream)
    (write-string text stream)
    (dotimes (i (- 30 (length text))) (write-char #\Space stream))
    (write-string " = " stream)
    (if boundp (prin1 value stream) (write-string "#<unbound slot>" stream))
    (terpri stream)))

(defmethod describe-object ((object standard-object) stream)
  (%describe-header object stream)
  (let ((class (class-of object)))
    (%ensure-finalized class)
    (dolist (allocation '(:instance :class))
      (let ((slots (remove allocation (%class-slots class)
                           :key #'%slot-definition-allocation :test-not #'eq)))
        (when slots
          (format stream "~%Slots with ~S allocation:~%" allocation)
          (dolist (slot slots)
            (let ((name (%slot-definition-name slot)))
              (if (slot-boundp object name)
                  (%describe-slot name t (slot-value object name) stream)
                  (%describe-slot name nil nil stream)))))))))

(defmethod describe-object ((object structure-object) stream)
  (%describe-header object stream)
  (let ((slots (%layout-slots (%structure-layout object))))
    (when slots
      (format stream "~%Slots with :INSTANCE allocation:~%")
      (dolist (slot slots)
        (%describe-slot (first slot) t (%structure-ref object (second slot)) stream)))))

;;; A class's place in the hierarchy and its own slots.
(defun %describe-class (class stream)
  (%ensure-finalized class)
  (format stream "  Class precedence-list: ~{~S~^, ~}~%"
          (mapcar #'class-name (%class-precedence-list class)))
  (format stream "  Direct superclasses: ~{~S~^, ~}~%"
          (mapcar #'class-name (%class-direct-superclasses class)))
  (if (%class-direct-subclasses class)
      (format stream "  Direct subclasses: ~{~S~^, ~}~%"
              (mapcar #'class-name (%class-direct-subclasses class)))
      (format stream "  No subclasses.~%"))
  (if (%class-direct-slots class)
      (progn
        (format stream "  Direct slots:~%")
        (dolist (slot (%class-direct-slots class))
          (format stream "    ~S~%" (%slot-definition-name slot))
          (if (%slot-definition-initargs slot)
              (format stream "      Initargs: ~{~S~^, ~}~%" (%slot-definition-initargs slot)))))
      (format stream "  No direct slots.~%")))

(defmethod describe-object ((object class) stream)
  (%describe-header object stream)
  (terpri stream)
  (%describe-class object stream))

(defmethod describe-object ((object generic-function) stream)
  (%describe-header object stream)
  (format stream "~%Lambda-list: ~S~%" (%gf-lambda-list object))
  (if (%gf-methods object)
      (progn
        (format stream "Methods:~%")
        (dolist (method (reverse (%gf-methods object)))
          (format stream "  ~{~S ~}~S~%"
                  (%method-qualifiers method)
                  (mapcar (lambda (specializer)
                            (if (%eql-specializer-p specializer)
                                (list 'eql (%eql-specializer-object specializer))
                                (class-name specializer)))
                          (%method-specializers method)))))
      (format stream "No methods.~%")))

;;; A symbol's meanings: as a variable, as a function, and as a class.
(defmethod describe-object ((object symbol) stream)
  (%describe-header object stream)
  (when (boundp object)
    (format stream "~%~S names a ~A variable:~%  Value: ~S~%"
            object (if (constantp object) "constant" "special") (symbol-value object)))
  (when (fboundp object)
    (format stream "~%~S names ~A~%" object
            (cond ((special-operator-p object) "a special operator.")
                  ((macro-function object) "a macro.")
                  ((%generic-function-p (fdefinition object))
                   (format nil "a generic function:~%  Lambda-list: ~S"
                           (%gf-lambda-list (fdefinition object))))
                  (t "a compiled function."))))
  (let ((class (find-class object nil)))
    (when class
      (format stream "~%~S names the ~(~A~) ~S:~%"
              object (symbol-name (class-name (class-of class))) class)
      (%describe-class class stream))))

(defmethod describe-object ((object array) stream)
  (%describe-header object stream)
  (if (vectorp object)
      (format stream "~%Element-type: ~S~%Length: ~D~%"
              (array-element-type object) (length object))
      (format stream "~%Element-type: ~S~%Dimensions: ~S~%"
              (array-element-type object) (array-dimensions object))))

(defmethod describe-object ((object hash-table) stream)
  (%describe-header object stream)
  (format stream "~%Test: ~S~%Count: ~D~%"
          (hash-table-test object) (hash-table-count object)))
