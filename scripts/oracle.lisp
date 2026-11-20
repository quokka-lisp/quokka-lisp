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
            (error () (format t "!! error~%"))))))))

(run-file (second sb-ext:*posix-argv*))
