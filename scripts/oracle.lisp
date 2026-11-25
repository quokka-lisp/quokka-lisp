;;;; Run a differential test file and print the canonical format.
;;;;
;;;; The SBCL half of the comparison. Reads one form at a time, evaluates it,
;;;; and prints either the value or a single error token — the same two lines
;;;; Quokka's `run_forms` produces.
;;;;
;;;; Printing has to match exactly, so two settings are forced rather than
;;;; inherited: pretty-printing off, because SBCL would otherwise insert line
;;;; breaks that turn one form's output into several lines, and upcased symbol
;;;; names, which is the default but worth not depending on.

(setf *print-pretty* nil)
(setf *print-case* :upcase)
(setf *print-readably* nil)

(defun run-file (path)
  (with-open-file (in path :direction :input)
    ;; A fresh cons as the end-of-file marker, so no value in the file can be
    ;; mistaken for it.
    (let ((eof (list :eof)))
      (loop
        (let ((form (handler-case (read in nil eof)
                      ;; A malformed file is a fault in the test rather than a
                      ;; result worth comparing.
                      (error (e)
                        (format *error-output* "cannot read ~a: ~a~%" path e)
                        (sb-ext:exit :code 1)))))
          (when (eq form eof) (return))
          (handler-case
              ;; Warnings are muffled, not suppressed globally: SBCL warns
              ;; about undefined variables at `setq`, which is correct and
              ;; says nothing about whether the answers agree.
              (let ((value (handler-bind ((warning #'muffle-warning))
                             (eval form))))
                (format t "=> ~S~%" value))
            ;; Which kind of error, when it is one of the types CLHS names:
            ;; `!! type-error`. Without the type every error looked alike, and
            ;; Quokka failing for the wrong reason agreed with SBCL signalling
            ;; the condition the test was written for (#103). The list is
            ;; `condition::REPORTED` in Rust, most specific first here, since
            ;; SBCL's own condition is usually a subclass of several.
            (error (e)
              (let ((type (find-if (lambda (type) (typep e type))
                                   '(division-by-zero floating-point-overflow
                                     floating-point-underflow floating-point-inexact
                                     floating-point-invalid-operation unbound-slot
                                     unbound-variable undefined-function end-of-file
                                     reader-error print-not-readable arithmetic-error
                                     cell-error control-error file-error package-error
                                     parse-error program-error stream-error type-error))))
                (format t "!! ~(~a~)~%" (or type 'error))))))))))

(run-file (second sb-ext:*posix-argv*))
