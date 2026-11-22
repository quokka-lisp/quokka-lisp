;;;; The printer, in Lisp (#46, step G of #39).
;;;;
;;;; Objects in, text out: the reader run backwards. Its one hard rule is that
;;;; what `prin1` writes, `read` reads back as the same object, or as an equal
;;;; one for things that cannot be the same, such as a list. For symbols that
;;;; means two things the Rust printer never had to know: a package prefix
;;;; when `*package*` would not find the symbol without one (CLHS 22.1.3.3.1),
;;;; and `|...|` around a name the reader would otherwise change, by upcasing
;;;; it, splitting it, or reading it as a number.
;;;;
;;;; Decided in #46 (Carsten):
;;;;
;;;; - `*standard-output*` writes to the host's console through a **host
;;;;   request**: each write leaves a `%write-console` request in the heap and
;;;;   the machine stops until the host has printed it, the same protocol as
;;;;   `%read-source` (#35). The first output Lisp has ever made.
;;;; - The compiled leg of the differential tests prints its results with
;;;;   this printer, so every file exercises it against SBCL.
;;;;
;;;; The Rust printer stays: the bootstrap and the interpreted leg have no
;;;; library, and print with it.
;;;;
;;;; SBCL's rules for what needs escaping were measured, not guessed, and are
;;;; in #46.

;;; ------------------------------------------------------------------------
;;; Output streams
;;;
;;; The second half of #42. Two kinds: a string output stream, which collects
;;; characters (backwards, in a list) until `get-output-stream-string` takes
;;; them; and the console, which sends each write to the host. Each remembers
;;; whether the last thing written ended a line, which is all `fresh-line`
;;; needs.

(defstruct %string-output-stream chars count fresh)
(defstruct %console-stream fresh)

(defun make-string-output-stream ()
  (make-%string-output-stream :chars nil :count 0 :fresh t))

;;; The text written so far, and the stream emptied for more.
(defun get-output-stream-string (stream)
  (let ((string (%chars-string (%string-output-stream-chars stream)
                               (%string-output-stream-count stream))))
    (setf (%string-output-stream-chars stream) nil)
    (setf (%string-output-stream-count stream) 0)
    string))

(defvar *standard-output* (make-%console-stream :fresh t))

;;; An output stream designator: nil and t both mean standard output. (CLHS
;;; makes t mean `*terminal-io*`, which Quokka does not have apart from the
;;; console.)
(defun %output-stream (designator)
  (cond ((or (null designator) (eq designator t)) *standard-output*)
        ((or (%string-output-stream-p designator) (%console-stream-p designator)) designator)
        (t (error "not an output stream"))))

(defun output-stream-p (stream)
  (or (%string-output-stream-p stream) (%console-stream-p stream)))
(defun input-stream-p (stream) (%string-input-stream-p stream))

;;; Every write comes down to this: characters START to END of STRING.
(defun %stream-write (stream string start end)
  (if (< start end)
      (progn
        (if (%console-stream-p stream)
            (progn
              (%write-console (if (and (eq start 0) (eq end (length string)))
                                  string
                                  (%substring string start end)))
              (setf (%console-stream-fresh stream) (eq (aref string (- end 1)) #\Newline)))
            (progn
              (setf (%string-output-stream-chars stream)
                    (%push-chars string start end (%string-output-stream-chars stream)))
              (setf (%string-output-stream-count stream)
                    (+ (%string-output-stream-count stream) (- end start)))
              (setf (%string-output-stream-fresh stream) (eq (aref string (- end 1)) #\Newline))))))
  string)

(defun %push-chars (string start end chars)
  (if (< start end)
      (%push-chars string (+ start 1) end (cons (aref string start) chars))
      chars))

(defun %fresh-p (stream)
  (if (%console-stream-p stream)
      (%console-stream-fresh stream)
      (%string-output-stream-fresh stream)))

(defun write-string (string &optional stream &key (start 0) end)
  (%stream-write (%output-stream stream) string start (if end end (length string)))
  string)

(defun write-line (string &optional stream &key (start 0) end)
  (let ((stream (%output-stream stream)))
    (%stream-write stream string start (if end end (length string)))
    (terpri stream)
    string))

(defun write-char (char &optional stream)
  (%stream-write (%output-stream stream) (make-string 1 :initial-element char) 0 1)
  char)

(defun terpri (&optional stream)
  (write-char #\Newline stream)
  nil)

;;; A newline unless the stream is already at the start of a line, and
;;; whether one was written.
(defun fresh-line (&optional stream)
  (let ((stream (%output-stream stream)))
    (if (%fresh-p stream) nil (progn (terpri stream) t))))

;;; Nothing is buffered — a console write is sent at once — so there is
;;; nothing to finish, force or clear.
(defun finish-output (&optional stream) nil)
(defun force-output (&optional stream) nil)
(defun clear-output (&optional stream) nil)

;;; `(with-output-to-string (var) body...)` — the text BODY writes to VAR.
;;; CLHS's optional string argument, writing into an existing string with a
;;; fill pointer, waits for fill pointers.
(defmacro with-output-to-string (binding &body body)
  (if (cdr binding) (error "with-output-to-string into a string is not supported yet"))
  `(let ((,(car binding) (make-string-output-stream)))
     ,@body
     (get-output-stream-string ,(car binding))))

;;; ------------------------------------------------------------------------
;;; Characters, by name

;;; Characters that print as themselves: everything but the controls.
(defun graphic-char-p (char)
  (let ((code (char-code char)))
    (or (and (> code 31) (< code 127)) (> code 159))))

;;; SBCL's names for the ASCII controls, by code, and for space and rubout.
;;; Above ASCII SBCL uses Unicode's names, which would need Unicode's name
;;; table; Quokka has none, so `char-name` answers nil there and the printer
;;; writes the character itself, which SBCL reads back just the same.
(defvar %control-names
  #("Nul" "Soh" "Stx" "Etx" "Eot" "Enq" "Ack" "Bel" "Backspace" "Tab" "Newline"
    "Vt" "Page" "Return" "So" "Si" "Dle" "Dc1" "Dc2" "Dc3" "Dc4" "Nak" "Syn"
    "Etb" "Can" "Em" "Sub" "Esc" "Fs" "Gs" "Rs" "Us" "Space"))

(defun char-name (char)
  (let ((code (char-code char)))
    (cond ((< code 33) (aref %control-names code))
          ((eq code 127) "Rubout")
          (t nil))))

;;; The character for digit WEIGHT in RADIX, or nil: `digit-char-p` backwards.
(defun digit-char (weight &optional (radix 10))
  (cond ((or (< weight 0) (not (< weight radix))) nil)
        ((< weight 10) (code-char (+ weight 48)))
        (t (code-char (+ weight 55)))))

;;; ------------------------------------------------------------------------
;;; Print variables (CLHS 22.1.1). Quokka prints on one line and does not
;;; label shared structure, so `*print-pretty*` and `*print-circle*` are not
;;; here: a variable that existed and was ignored would be worse than none.

(defvar *print-escape* t)
(defvar *print-base* 10)
(defvar *print-radix* nil)
(defvar *print-case* :upcase)
(defvar *print-length* nil)
(defvar *print-level* nil)
(defvar *print-gensym* t)

;;; ------------------------------------------------------------------------
;;; The entry points

(defun write-to-string (object &key (escape *print-escape*) (base *print-base*)
                                    (radix *print-radix*) (case *print-case*)
                                    (length *print-length*) (level *print-level*)
                                    (gensym *print-gensym*))
  (let ((*print-escape* escape) (*print-base* base) (*print-radix* radix)
        (*print-case* case) (*print-length* length) (*print-level* level)
        (*print-gensym* gensym))
    (let ((out (make-string-output-stream)))
      (%print-object object out 0)
      (get-output-stream-string out))))

(defun prin1-to-string (object) (write-to-string object :escape t))
(defun princ-to-string (object) (write-to-string object :escape nil))

;;; Each builds the whole text first and writes it once, so printing to the
;;; console is one request to the host, not one per character.
(defun write (object &key stream (escape *print-escape*) (base *print-base*)
                          (radix *print-radix*) (case *print-case*)
                          (length *print-length*) (level *print-level*)
                          (gensym *print-gensym*))
  (write-string (write-to-string object :escape escape :base base :radix radix
                                        :case case :length length :level level
                                        :gensym gensym)
                stream)
  object)

(defun prin1 (object &optional stream)
  (write-string (prin1-to-string object) stream)
  object)

(defun princ (object &optional stream)
  (write-string (princ-to-string object) stream)
  object)

;;; A newline, the object as `prin1` writes it, and a space.
(defun print (object &optional stream)
  (write-string (%string-append (%string-append (string #\Newline) (prin1-to-string object)) " ")
                stream)
  object)

;;; `(string x)` — a string designator as a string: a string, a symbol's
;;; name, or a character on its own.
(defun string (designator)
  (cond ((stringp designator) designator)
        ((symbolp designator) (symbol-name designator))
        ((characterp designator) (make-string 1 :initial-element designator))
        (t (error "not a string designator"))))

;;; ------------------------------------------------------------------------
;;; Objects

(defun %print-object (object out level)
  (cond ((integerp object) (%print-integer object out))
        ((characterp object) (%print-character object out))
        ((symbolp object) (%print-symbol object out))
        ((stringp object) (%print-string object out))
        ((consp object) (%print-list object out level))
        ((vectorp object) (%print-vector object out level))
        ;; Functions, conditions, hash tables: nothing reads them back, so
        ;; CLHS lets each implementation print them its own way. The Rust
        ;; printer's way, so that both printers agree.
        (t (write-string (%printed object) out))))

;;; `*print-level*`: deeper than this, a list or vector is just `#`.
(defun %too-deep-p (level) (and *print-level* (not (< level *print-level*))))

;;; `*print-length*`: after this many elements, `...`.
(defun %too-long-p (count) (and *print-length* (not (< count *print-length*))))

(defun %print-list (list out level)
  (if (%too-deep-p level)
      (write-char #\# out)
      (progn (write-char #\( out)
             (%print-list-items list out (+ level 1) 0)
             (write-char #\) out))))

(defun %print-list-items (list out level count)
  (if (%too-long-p count)
      (write-string "..." out)
      (progn
        (%print-object (car list) out level)
        (let ((rest (cdr list)))
          (cond ((null rest) nil)
                ((consp rest)
                 (write-char #\Space out)
                 (%print-list-items rest out level (+ count 1)))
                ;; An improper list: the dot is why a cons is a pair.
                (t (write-string " . " out)
                   (%print-object rest out level)))))))

(defun %print-vector (vector out level)
  (if (%too-deep-p level)
      (write-char #\# out)
      (progn (write-string "#(" out)
             (%print-vector-items vector out (+ level 1) 0 (length vector))
             (write-char #\) out))))

(defun %print-vector-items (vector out level index end)
  (if (< index end)
      (progn
        (if (> index 0) (write-char #\Space out))
        (if (%too-long-p index)
            (write-string "..." out)
            (progn (%print-object (aref vector index) out level)
                   (%print-vector-items vector out level (+ index 1) end))))))

;;; With escapes, in double quotes with `"` and `\` backslashed: the only two
;;; escapes a Lisp string has. Without, the characters alone.
(defun %print-string (string out)
  (if *print-escape*
      (progn (write-char #\" out)
             (%print-escaped string out 0 (length string) #\")
             (write-char #\" out))
      (write-string string out)))

(defun %print-escaped (string out at end quote)
  (if (< at end)
      (let ((char (aref string at)))
        (if (or (eq char quote) (eq char #\\)) (write-char #\\ out))
        (write-char char out)
        (%print-escaped string out (+ at 1) end quote))))

;;; `#\a`, `#\Tab`, and `#\ ` for a space, as SBCL writes it.
(defun %print-character (char out)
  (if *print-escape*
      (progn (write-string "#\\" out)
             (if (graphic-char-p char)
                 (write-char char out)
                 (let ((name (char-name char)))
                   (if name (write-string name out) (write-char char out)))))
      (write-char char out)))

;;; ------------------------------------------------------------------------
;;; Integers, in `*print-base*`

(defun %print-integer (integer out)
  (if (and *print-radix* (not (eq *print-base* 10)))
      (write-string (%radix-prefix *print-base*) out))
  (if (< integer 0) (write-char #\- out))
  (%print-digits (%integer-digits integer *print-base* nil) out)
  (if (and *print-radix* (eq *print-base* 10)) (write-char #\. out)))

(defun %radix-prefix (base)
  (cond ((eq base 2) "#b")
        ((eq base 8) "#o")
        ((eq base 16) "#x")
        (t (let ((*print-base* 10) (*print-radix* nil))
             (%string-append "#" (%string-append (prin1-to-string base) "r"))))))

;;; Digit weights, most significant first. A negative number is divided as it
;;; is, with each remainder's sign dropped: negating it first would overflow
;;; for the most negative fixnum, which has no positive twin.
(defun %integer-digits (integer base digits)
  (let ((quotient (%quotient integer base))
        (digits (cons (abs (rem integer base)) digits)))
    (if (eq quotient 0) digits (%integer-digits quotient base digits))))

(defun %print-digits (digits out)
  (if digits
      (progn (write-char (digit-char (car digits) 36) out)
             (%print-digits (cdr digits) out))))

;;; ------------------------------------------------------------------------
;;; Symbols

(defun %print-symbol (symbol out)
  (let ((name (symbol-name symbol)))
    (if *print-escape* (%print-prefix symbol name out))
    (%print-name name out)))

;;; With escapes, a name the reader would change goes in bars; otherwise it
;;; is written in `*print-case*`.
(defun %print-name (name out)
  (if (and *print-escape* (%needs-bars-p name))
      (progn (write-char #\| out)
             (%print-escaped name out 0 (length name) #\|)
             (write-char #\| out))
      (%print-cased name out)))

;;; Where the symbol comes from, if `*package*` would not find it alone:
;;; `:` for a keyword, `#:` for no package at all, and otherwise the home
;;; package's name with one colon if the symbol is external there, two if
;;; not.
(defun %print-prefix (symbol name out)
  (let ((home (symbol-package symbol)))
    (cond ((null home) (if *print-gensym* (write-string "#:" out)))
          ((eq home %keyword-package) (write-char #\: out))
          ((%accessible-as-itself-p symbol name) nil)
          (t (%print-name (package-name home) out)
             (write-string (if (eq (nth-value 1 (find-symbol name home)) :external) ":" "::")
                           out)))))

(defun %accessible-as-itself-p (symbol name)
  (multiple-value-bind (found status) (find-symbol name *package*)
    (and status (eq found symbol))))

;;; Would the reader read NAME back as something else? It would if NAME is
;;; empty, all dots, has the syntax of a number in `*print-base*`, or
;;; contains a lower-case letter (upcased), whitespace, a macro character, a
;;; colon or an escape. SBCL counts `#` anywhere, though the reader only
;;; dispatches on it at the start of a token; matching SBCL wins.
(defun %needs-bars-p (name)
  (let ((end (length name)))
    (or (eq end 0)
        (%all-dots-p name 0 end)
        (%token-integer name)
        (%unsupported-number-p name)
        (and (not (eq *print-base* 10)) (%digits-in-base-p name *print-base*))
        (%special-char-in-p name 0 end))))

(defun %digits-in-base-p (name base)
  (let* ((end (length name))
         (start (%skip-sign name 0 end)))
    (and (< start end) (%all-digits-in-base-p name start end base))))

(defun %all-digits-in-base-p (name at end base)
  (if (< at end)
      (and (digit-char-p (aref name at) base) (%all-digits-in-base-p name (+ at 1) end base))
      t))

(defun %special-char-in-p (name at end)
  (and (< at end)
       (let ((char (aref name at)))
         (or (lower-case-p char)
             (%whitespacep char)
             (member char '(#\| #\\ #\:))
             (gethash char (%readtable-macros (%the-readtable *readtable*)))
             (%special-char-in-p name (+ at 1) end)))))

;;; `*print-case*` changes only upper-case letters: :downcase lowers them all;
;;; :capitalize keeps the first letter of each word — a run of letters and
;;; digits — and lowers the rest. `FOO-BAR` becomes `Foo-Bar`, `X2Y` `X2y`.
(defun %print-cased (name out)
  (cond ((eq *print-case* :downcase) (%print-downcased name out 0 (length name)))
        ((eq *print-case* :capitalize) (%print-capitalized name out 0 (length name) t))
        (t (write-string name out))))

(defun %print-downcased (name out at end)
  (if (< at end)
      (progn (write-char (char-downcase (aref name at)) out)
             (%print-downcased name out (+ at 1) end))))

(defun %print-capitalized (name out at end word-start)
  (if (< at end)
      (let ((char (aref name at)))
        (write-char (if word-start char (char-downcase char)) out)
        (%print-capitalized name out (+ at 1) end (not (alphanumericp char))))))
