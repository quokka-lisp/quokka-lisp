;;; Gray streams (phase 4 step B, #136): streams a program defines, in
;;; QUOKKA-GRAY as SBCL's are in SB-GRAY. Tier 1: compiled and SBCL only. The
;;; file runs in a package that uses whichever Gray package the Lisp has.

(progn (defpackage :gray-user (:use :common-lisp #+sbcl :sb-gray #+quokka :quokka-gray)) :ok)
(progn (in-package :gray-user) :ok)

;; An output stream that keeps what it is given, upcased, and counts its
;; column: the two methods the proposal requires, and nothing else.
(progn (defclass shouting-stream (fundamental-character-output-stream)
  ((text :initform (make-string-output-stream) :reader text)
   (column :initform 0 :accessor column))) :ok)

(progn (defmethod stream-write-char ((s shouting-stream) char)
  (write-char (char-upcase char) (text s))
  (setf (column s) (if (char= char #\Newline) 0 (+ (column s) 1)))
  char) :ok)

(progn (defmethod stream-line-column ((s shouting-stream)) (column s)) :ok)

(defun shout (stream) (get-output-stream-string (text stream)))

(defvar *loud* (make-instance 'shouting-stream))

(streamp *loud*)
(typep *loud* 'stream)
(output-stream-p *loud*)
(input-stream-p *loud*)
(open-stream-p *loud*)
(stream-element-type *loud*)
(interactive-stream-p *loud*)

;; The standard functions reach the stream through the defaults.
(write-char #\a *loud*)
(write-string "bcd" *loud* :start 1)
(progn (terpri *loud*) (write-line "hello" *loud*) (shout *loud*))
(progn (write-string "x" *loud*) (list (fresh-line *loud*) (fresh-line *loud*) (shout *loud*)))
(progn (format *loud* "~A and ~S~%" "text" :key) (shout *loud*))
(progn (print '(1 "two" #\3) *loud*) (shout *loud*))
(progn (prin1 (list 1.5 'sym "s") *loud*) (princ " ok" *loud*) (shout *loud*))
(let ((*standard-output* *loud*)) (format t "~D cards" 3) (shout *loud*))
(progn (write-sequence '(#\q #\r #\s) *loud*) (shout *loud*))
(list (finish-output *loud*) (force-output *loud*) (clear-output *loud*))
(progn (write-string "ab" *loud*) (stream-advance-to-column *loud* 6) (write-char #\| *loud*) (shout *loud*))

;; Closing.
(close *loud*)
(open-stream-p *loud*)
(with-open-stream (s (make-instance 'shouting-stream)) (write-string "in" s) (shout s))

;; A stream that writes whole strings at once, overriding the default.
(progn (defclass run-stream (fundamental-character-output-stream)
  ((runs :initform nil :accessor runs))) :ok)
(progn (defmethod stream-write-char ((s run-stream) char)
  (push (string char) (runs s)) char) :ok)
(progn (defmethod stream-write-string ((s run-stream) string &optional (start 0) end)
  (push (subseq string start end) (runs s)) string) :ok)
(progn (defmethod stream-line-column ((s run-stream)) nil) :ok)
(let ((s (make-instance 'run-stream)))
  (write-string "whole" s)
  (write-char #\! s)
  (reverse (runs s)))

;; An input stream over a list of lines.
(progn (defclass lines-stream (fundamental-character-input-stream)
  ((text :initarg :text :reader lines-text)
   (at :initform 0 :accessor lines-at))) :ok)
(progn (defmethod stream-read-char ((s lines-stream))
  (if (< (lines-at s) (length (lines-text s)))
      (prog1 (char (lines-text s) (lines-at s)) (incf (lines-at s)))
      :eof)) :ok)
(progn (defmethod stream-unread-char ((s lines-stream) char)
  (decf (lines-at s)) nil) :ok)

(defun lines (&rest lines)
  (make-instance 'lines-stream
                 :text (format nil "~{~A~^~%~}" lines)))

(input-stream-p (lines "a"))
(output-stream-p (lines "a"))
(let ((s (lines "first line" "second"))) (list (read-char s) (peek-char nil s) (read-char s)))
(let ((s (lines "first line" "second")))
  (multiple-value-list (read-line s)))
(let ((s (lines "first line" "second")))
  (read-line s)
  (multiple-value-list (read-line s)))
(let ((s (lines "only")))
  (read-line s)
  (list (read-line s nil :done) (read-char s nil :end)))
(let ((s (lines "(+ 1 2) foo \"bar\" #(1 2)")))
  (list (read s) (read s) (read s) (read s) (read s nil :eof)))
(let ((s (lines "abc"))) (list (listen s) (read-char-no-hang s) (progn (read-line s) (listen s))))
(let ((s (lines "  x"))) (peek-char t s))
(let ((s (lines "hello")) (v (make-string 3)))
  (list (read-sequence v s) v))
(let ((s (lines "hello")) (v (make-string 8 :initial-element #\-)))
  (list (read-sequence v s :start 2) v))
(let ((*standard-input* (lines "42 rest"))) (list (read) (read-line)))
(with-input-from-string (in "plain") (read-line in))

;; Errors as SBCL signals them.
(handler-case (read-char (make-instance 'fundamental-character-input-stream))
  (error (c) (not (null c))))
(handler-case (write-char #\a 42) (type-error () :type-error))
(handler-case (streamp 42) (error () :error))
