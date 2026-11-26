;;; Conditions as objects (phase 3 step B, sub-step 1, #125): condition
;;; classes, define-condition, make-condition, slots and readers, reports,
;;; and condition types as classes in typep and subtypep.

(define-condition dc-error (error)
  ((code :initarg :code :reader dc-error-code :initform 0)
   (note :initarg :note :accessor dc-error-note))
  (:report (lambda (condition stream)
             (format stream "dc-error ~A" (dc-error-code condition)))))
(define-condition dc-sub (dc-error)
  ((extra :initarg :extra :initform :none :reader dc-sub-extra))
  (:default-initargs :code 9))
(define-condition dc-warning (warning) () (:report "a dc warning"))
(defun dc-report (condition stream)
  (format stream "named report ~A" (dc-error-code condition)))
(define-condition dc-named (dc-error) () (:report dc-report))
(define-condition dc-plain (error) ())

;; make-condition: initargs, initforms, default initargs, readers, writers.
(let ((c (make-condition 'dc-error :code 3 :note "n")))
  (list (dc-error-code c) (dc-error-note c) (typep c 'dc-error) (typep c 'error)
        (typep c 'serious-condition) (typep c 'condition) (typep c 'warning)
        (typep c 'standard-object)))
(let ((c (make-condition 'dc-sub)))
  (list (dc-error-code c) (dc-sub-extra c) (typep c 'dc-error) (class-name (class-of c)) (type-of c)))
(dc-error-code (make-condition 'dc-sub :code 2))
(let ((c (make-condition 'dc-error))) (setf (dc-error-note c) 'changed) (dc-error-note c))
(handler-case (dc-error-note (make-condition 'dc-error)) (unbound-slot () :unbound))
(slot-value (make-condition 'dc-error :code 4) 'code)
(slot-boundp (make-condition 'dc-error) 'note)
(dc-error-code (make-instance 'dc-error :code 8))

;; Reports: what princ prints, never what prin1 does.
(princ-to-string (make-condition 'dc-error :code 5))
(princ-to-string (make-condition 'dc-sub))
(princ-to-string (make-condition 'dc-warning))
(princ-to-string (make-condition 'dc-named :code 6))
(format nil "~A" (make-condition 'simple-error :format-control "a ~A b ~S" :format-arguments '(1 "x")))
(let ((c (make-condition 'simple-warning :format-control "w ~D" :format-arguments '(7))))
  (list (simple-condition-format-control c) (simple-condition-format-arguments c)
        (princ-to-string c)))
(string= (subseq (prin1-to-string (make-condition 'dc-error)) 0 2) "#<")
(let ((*print-escape* nil)) (write-to-string (make-condition 'dc-error :code 1)))

;; The standard condition types' slots, through CLHS's readers.
(let ((c (make-condition 'type-error :datum 5 :expected-type 'list)))
  (list (type-error-datum c) (type-error-expected-type c)))
(cell-error-name (make-condition 'unbound-variable :name 'foo))
(cell-error-name (make-condition 'undefined-function :name 'bar))
(let ((c (make-condition 'unbound-slot :name 's :instance 'i)))
  (list (cell-error-name c) (unbound-slot-instance c)))
(package-error-package (make-condition 'package-error :package "CL"))
(print-not-readable-object (make-condition 'print-not-readable :object 3))
(let ((c (make-condition 'division-by-zero :operation '/ :operands '(1 0))))
  (list (arithmetic-error-operation c) (arithmetic-error-operands c)))
(stream-error-stream (make-condition 'end-of-file :stream nil))
(let ((c (make-condition 'simple-type-error :datum 1 :expected-type 'string
                         :format-control "x" :format-arguments nil)))
  (list (type-error-datum c) (simple-condition-format-control c)
        (typep c 'simple-condition) (typep c 'type-error) (typep c 'simple-error)))

;; Condition types are classes.
(subtypep 'dc-sub 'dc-error)
(subtypep 'dc-error 'error)
(subtypep 'dc-error 'warning)
(subtypep 'dc-warning 'condition)
(subtypep 'simple-type-error 'type-error)
(subtypep 'type-error 'simple-type-error)
(subtypep 'dc-error '(or warning dc-sub))
(subtypep 'error 'standard-object)
(subtypep '(and dc-error warning) nil)
(typep (make-condition 'dc-sub) '(and error (not warning)))
(typep (find-class 'dc-error) (class-of (find-class 'error)))
(eq (find-class 'dc-error) (class-of (make-condition 'dc-error)))
(typep (find-class 'error) 'class)

;; What make-condition refuses, and an initarg it ignores, as SBCL does.
(handler-case (make-condition 'no-such-condition-type) (error () :error))
(handler-case (make-condition 'standard-object) (error () :error))
(typep (make-condition 'dc-plain :no-such-initarg 1) 'dc-plain)

;; A redefinition keeps the class.
(let ((before (find-class 'dc-plain)))
  (define-condition dc-plain (error) ((later :initarg :later :reader dc-plain-later)))
  (list (eq before (find-class 'dc-plain)) (dc-plain-later (make-condition 'dc-plain :later 1))))

;; The machine's errors are still caught, and of their types.
(handler-case (car 5) (type-error (c) (list (typep c 'error) (typep c 'type-error))))
(handler-case (car 5) (error (c) (typep c 'condition)))
