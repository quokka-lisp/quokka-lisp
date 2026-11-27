;;;; Gray streams (#136, phase 4 step B; docs/display.md, decision 9).
;;;;
;;;; Until now a stream was one of three structures the library made itself —
;;;; a string input stream, a string output stream, the console — and every
;;;; stream function asked which of the three it had. Nothing let a program
;;;; make a stream of its own, and phase 4 needs exactly that: a viewer's
;;;; stream, which `format` and `print` write to and the listener reads
;;;; from, is a class the toolkit defines.
;;;;
;;;; Common Lisp's standard has no protocol for defining a stream. David Gray
;;;; proposed one in 1989 (X3J13 issue STREAM-DEFINITION-BY-USER), the
;;;; committee did not adopt it, and every implementation since has provided
;;;; it anyway: SBCL in `SB-GRAY`, CCL, LispWorks, Allegro, ECL. A stream is an
;;;; instance of a subclass of `fundamental-stream`, and the standard
;;;; functions call generic functions on it: `write-char` calls
;;;; `stream-write-char`, `read-char` calls `stream-read-char`, and so on.
;;;; A class need define only the few methods the proposal requires
;;;; (`stream-write-char` and `stream-line-column` for character output,
;;;; `stream-read-char` and `stream-unread-char` for character input); every
;;;; other function has a default method built on those.
;;;;
;;;; The names are SBCL's, in the package `QUOKKA-GRAY` (`packages.lisp`), so
;;;; that a test can define a stream class on both and compare.
;;;;
;;;; The structure streams keep their fast paths: the reader and printer ask
;;;; first whether they have one of the three, and only a stream that is
;;;; none of them goes through a generic function (`%gray-stream-p`).

(defclass fundamental-stream (standard-object stream)
  ((%open-p :initform t)))
(defclass fundamental-input-stream (fundamental-stream) ())
(defclass fundamental-output-stream (fundamental-stream) ())
(defclass fundamental-character-stream (fundamental-stream) ())
(defclass fundamental-binary-stream (fundamental-stream) ())
(defclass fundamental-character-input-stream
    (fundamental-input-stream fundamental-character-stream) ())
(defclass fundamental-character-output-stream
    (fundamental-output-stream fundamental-character-stream) ())
(defclass fundamental-binary-input-stream
    (fundamental-input-stream fundamental-binary-stream) ())
(defclass fundamental-binary-output-stream
    (fundamental-output-stream fundamental-binary-stream) ())

;;; Is OBJECT a stream a program defined? The class is looked up once, here:
;;; a warm rebuild refills the class rather than making a new one, so the
;;; object this holds stays the class.
(defvar %fundamental-stream-class (find-class 'fundamental-stream))

(defun %gray-stream-p (object)
  (and (%instance-wrapper object)
       (%class-typep object %fundamental-stream-class)))

;;; The library's three structure streams.
(defun %structure-stream-p (object)
  (or (%string-input-stream-p object) (%string-output-stream-p object)
      (%console-stream-p object)))

(defun streamp (object)
  (or (%structure-stream-p object) (%gray-stream-p object)))

(defun %not-a-stream (object)
  (error 'type-error :datum object :expected-type 'stream))

;;; ------------------------------------------------------------------------
;;; The standard functions that are generic in SBCL
;;;
;;; `close`, `open-stream-p`, `input-stream-p`, `output-stream-p`,
;;; `interactive-stream-p` and `stream-element-type` are generic functions,
;;; as in SBCL, so that a stream class can answer them; the method on `t`
;;; answers for the structure streams and refuses anything else.

(defgeneric close (stream &key abort))
(defmethod close ((stream t) &key abort)
  (declare (ignore abort))
  (if (%structure-stream-p stream) t (%not-a-stream stream)))
(defmethod close ((stream fundamental-stream) &key abort)
  (declare (ignore abort))
  (setf (slot-value stream '%open-p) nil)
  t)

(defgeneric open-stream-p (stream))
(defmethod open-stream-p ((stream t))
  (if (%structure-stream-p stream) t (%not-a-stream stream)))
(defmethod open-stream-p ((stream fundamental-stream))
  (slot-value stream '%open-p))

(defgeneric input-stream-p (stream))
(defmethod input-stream-p ((stream t))
  (cond ((%string-input-stream-p stream) t)
        ((%structure-stream-p stream) (eq stream %console-io))
        (t (%not-a-stream stream))))
(defmethod input-stream-p ((stream fundamental-stream)) nil)
(defmethod input-stream-p ((stream fundamental-input-stream)) t)

(defgeneric output-stream-p (stream))
(defmethod output-stream-p ((stream t))
  (cond ((%string-input-stream-p stream) nil)
        ((%structure-stream-p stream) t)
        (t (%not-a-stream stream))))
(defmethod output-stream-p ((stream fundamental-stream)) nil)
(defmethod output-stream-p ((stream fundamental-output-stream)) t)

(defgeneric interactive-stream-p (stream))
(defmethod interactive-stream-p ((stream t))
  (cond ((%console-stream-p stream) t)
        ((eq stream %console-input) t)
        ((%structure-stream-p stream) nil)
        (t (%not-a-stream stream))))
(defmethod interactive-stream-p ((stream fundamental-stream)) nil)

(defgeneric stream-element-type (stream))
(defmethod stream-element-type ((stream t))
  (if (%structure-stream-p stream) 'character (%not-a-stream stream)))
(defmethod stream-element-type ((stream fundamental-character-stream)) 'character)

;;; ------------------------------------------------------------------------
;;; Character input
;;;
;;; `stream-read-char` returns a character or `:eof`; `stream-unread-char`
;;; puts back the one just read. The rest have defaults built on those two.

(defgeneric stream-read-char (stream))
(defgeneric stream-unread-char (stream character))

(defgeneric stream-read-char-no-hang (stream))
(defmethod stream-read-char-no-hang ((stream fundamental-character-input-stream))
  (stream-read-char stream))

(defgeneric stream-peek-char (stream))
(defmethod stream-peek-char ((stream fundamental-character-input-stream))
  (let ((char (stream-read-char stream)))
    (unless (eq char :eof) (stream-unread-char stream char))
    char))

(defgeneric stream-listen (stream))
(defmethod stream-listen ((stream fundamental-character-input-stream))
  (let ((char (stream-read-char-no-hang stream)))
    (if (or (null char) (eq char :eof))
        nil
        (progn (stream-unread-char stream char) t))))

;;; The characters up to the next newline, and whether the input ended
;;; before one did.
(defgeneric stream-read-line (stream))
(defmethod stream-read-line ((stream fundamental-character-input-stream))
  (let ((out (make-string-output-stream)))
    (do ((char (stream-read-char stream) (stream-read-char stream)))
        ((or (eq char :eof) (eql char #\Newline))
         (values (get-output-stream-string out) (eq char :eof)))
      (write-char char out))))

(defgeneric stream-clear-input (stream))
(defmethod stream-clear-input ((stream fundamental-input-stream)) nil)

;;; ------------------------------------------------------------------------
;;; Character output
;;;
;;; `stream-write-char` writes a character, and `stream-line-column` says how
;;; far along the current line the stream is, or nil if it cannot say. The
;;; rest have defaults built on those two.

(defgeneric stream-write-char (stream character))

(defgeneric stream-line-column (stream))
(defmethod stream-line-column ((stream fundamental-character-output-stream)) nil)

(defgeneric stream-start-line-p (stream))
(defmethod stream-start-line-p ((stream fundamental-character-output-stream))
  (eql (stream-line-column stream) 0))

(defgeneric stream-write-string (stream string &optional start end))
(defmethod stream-write-string ((stream fundamental-character-output-stream) string
                                &optional (start 0) end)
  (let ((end (or end (length string))))
    (do ((i start (+ i 1)))
        ((>= i end) string)
      (stream-write-char stream (char string i)))))

(defgeneric stream-terpri (stream))
(defmethod stream-terpri ((stream fundamental-character-output-stream))
  (stream-write-char stream #\Newline)
  nil)

;;; A newline unless the stream is at the start of a line, and whether one
;;; was written.
(defgeneric stream-fresh-line (stream))
(defmethod stream-fresh-line ((stream fundamental-character-output-stream))
  (if (stream-start-line-p stream)
      nil
      (progn (stream-terpri stream) t)))

;;; Spaces up to COLUMN, if the stream knows where it is; true if it did.
(defgeneric stream-advance-to-column (stream column))
(defmethod stream-advance-to-column ((stream fundamental-character-output-stream) column)
  (let ((now (stream-line-column stream)))
    (when now
      (dotimes (i (- column now)) (stream-write-char stream #\Space))
      t)))

(defgeneric stream-line-length (stream))
(defmethod stream-line-length ((stream fundamental-character-output-stream)) nil)

(defgeneric stream-finish-output (stream))
(defmethod stream-finish-output ((stream fundamental-output-stream)) nil)
(defgeneric stream-force-output (stream))
(defmethod stream-force-output ((stream fundamental-output-stream)) nil)
(defgeneric stream-clear-output (stream))
(defmethod stream-clear-output ((stream fundamental-output-stream)) nil)

;;; ------------------------------------------------------------------------
;;; Binary streams and sequences
;;;
;;; Quokka has no binary streams of its own yet (files come with file hosts),
;;; but the protocol is the proposal's, so a program can define one.

(defgeneric stream-read-byte (stream))
(defgeneric stream-write-byte (stream integer))

(defgeneric stream-read-sequence (stream sequence &optional start end))
(defmethod stream-read-sequence ((stream fundamental-character-input-stream) sequence
                                 &optional (start 0) end)
  (%read-sequence-by stream sequence start end #'stream-read-char))
(defmethod stream-read-sequence ((stream fundamental-binary-input-stream) sequence
                                 &optional (start 0) end)
  (%read-sequence-by stream sequence start end #'stream-read-byte))

(defun %read-sequence-by (stream sequence start end reader)
  (let ((end (or end (length sequence))))
    (do ((i start (+ i 1)))
        ((>= i end) i)
      (let ((item (funcall reader stream)))
        (if (eq item :eof)
            (return i)
            (setf (elt sequence i) item))))))

(defgeneric stream-write-sequence (stream sequence &optional start end))
(defmethod stream-write-sequence ((stream fundamental-character-output-stream) sequence
                                  &optional (start 0) end)
  (let ((end (or end (length sequence))))
    (do ((i start (+ i 1)))
        ((>= i end) sequence)
      (stream-write-char stream (elt sequence i)))))
(defmethod stream-write-sequence ((stream fundamental-binary-output-stream) sequence
                                  &optional (start 0) end)
  (let ((end (or end (length sequence))))
    (do ((i start (+ i 1)))
        ((>= i end) sequence)
      (stream-write-byte stream (elt sequence i)))))

(defgeneric stream-file-position (stream &optional position))
(defmethod stream-file-position ((stream fundamental-stream) &optional position)
  (declare (ignore position))
  nil)

;;; ------------------------------------------------------------------------
;;; Standard functions that reach the protocol only through it

(defun read-byte (stream &optional (eof-error-p t) eof-value)
  (let ((byte (if (%gray-stream-p stream) (stream-read-byte stream) (%not-a-binary stream))))
    (if (eq byte :eof) (%end-of-file eof-error-p eof-value) byte)))

(defun write-byte (byte stream)
  (if (%gray-stream-p stream) (stream-write-byte stream byte) (%not-a-binary stream))
  byte)

(defun %not-a-binary (stream)
  (error 'type-error :datum stream :expected-type '(satisfies binary-stream)))

;;; `(with-open-stream (var stream) body...)`: BODY with VAR bound to STREAM,
;;; which is closed however BODY is left.
(defmacro with-open-stream (binding &body body)
  (let ((var (car binding)))
    `(let ((,var ,(cadr binding)))
       (unwind-protect (progn ,@body)
         (when ,var (close ,var))))))

;;; `read-sequence` and `write-sequence` (CLHS 21.2): a Gray stream is asked
;;; for the whole run; the library's own streams go a character at a time.
(defun read-sequence (sequence stream &key (start 0) end)
  (let ((stream (%input-stream stream)))
    (if (%gray-stream-p stream)
        (stream-read-sequence stream sequence start end)
        (%read-sequence-by stream sequence start end
                           (lambda (s) (read-char s nil :eof))))))

(defun write-sequence (sequence stream &key (start 0) end)
  (let ((stream (%output-stream stream)))
    (if (%gray-stream-p stream)
        (stream-write-sequence stream sequence start end)
        (let ((end (or end (length sequence))))
          (do ((i start (+ i 1)))
              ((>= i end))
            (write-char (elt sequence i) stream))))
    sequence))
