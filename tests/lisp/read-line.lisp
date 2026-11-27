;;; read-line on string streams (#131): the line without its newline, and
;;; whether the input ended before a newline did.

(with-input-from-string (s (format nil "one~%two~%~%three"))
  (list (multiple-value-list (read-line s))
        (multiple-value-list (read-line s))
        (multiple-value-list (read-line s))
        (multiple-value-list (read-line s))
        (multiple-value-list (read-line s nil :end))))

(with-input-from-string (s "")
  (read-line s nil 'nothing))

(with-input-from-string (s "")
  (handler-case (read-line s) (end-of-file () :eof)))

(with-input-from-string (s "abc")
  (list (read-char s) (read-line s)))

(with-input-from-string (s (format nil "x~%"))
  (list (read-line s) (read-line s nil :done)))
