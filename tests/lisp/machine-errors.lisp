;;; The machine's own errors signalled through Lisp (phase 3 step B,
;;; sub-step 3, #125): a handler-bind sees them, they carry the initargs
;;; CLHS gives their types, and a running handler's floor is respected.

;; A type-error from a primitive knows its datum and expected type.
(handler-case (car 5) (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (+ 1 'a) (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (symbol-name 3) (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (char-code "a") (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (funcall 5) (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (aref (vector 1 2) 5) (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))

;; Cell errors know the name.
(handler-case mx-unbound-variable (unbound-variable (c) (cell-error-name c)))
(handler-case (mx-undefined-function 1) (undefined-function (c) (cell-error-name c)))
(handler-case (funcall 'mx-undefined-function) (undefined-function (c) (cell-error-name c)))
(handler-case (symbol-value 'mx-unbound-variable) (unbound-variable (c) (cell-error-name c)))

;; Arithmetic errors know the operation.
(handler-case (/ 1 0) (division-by-zero (c) (list (arithmetic-error-operation c) (arithmetic-error-operands c))))

;; handler-bind sees a machine error where it happened.
(handler-bind ((type-error (lambda (c) (throw :mx (list :bind (type-error-datum c))))))
  (catch :mx (car 'mx-b)))
(let ((seen nil))
  (list (handler-case
            (handler-bind ((error (lambda (c) (push (typep c 'type-error) seen))))
              (car 5))
          (type-error () :case))
        seen))

;; A machine error inside a handler goes past the handler-cases inside it.
(handler-case
    (handler-bind ((condition (lambda (c) c (car 5))))
      (handler-case (signal "hi") (type-error () :wrong)))
  (type-error () :right))

;; An unwanted machine error goes further out, through a handler-case
;; that does not want it.
(handler-case (handler-case (car 5) (unbound-variable () :inner)) (type-error () :outer))
