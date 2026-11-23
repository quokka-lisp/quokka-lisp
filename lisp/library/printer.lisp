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
              (%write-console (if (and (eq start 0) (eq end (length string))
                                       (simple-string-p string))
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
;;; Print variables (CLHS 22.1.1).
;;;
;;; #46 left out the ones for features Quokka lacked, on the grounds that a
;;; variable that existed and was ignored would be worse than none. Step Q
;;; (#64) reversed that: CLHS makes all of them part of every program's world
;;; — `with-standard-io-syntax` binds them, and code that only binds or reads
;;; them broke without them although it printed exactly what Quokka prints.
;;; So every one exists now. Those of the pretty printer, which is tier 2,
;;; hold their values and are not yet consulted: `*print-pretty*`,
;;; `*print-lines*`, `*print-miser-width*`, `*print-right-margin*` and
;;; `*print-pprint-dispatch*`, which will hold a dispatch table and holds nil
;;; until there are any.

(defvar *print-escape* t)
(defvar *print-base* 10)
(defvar *print-radix* nil)
(defvar *print-case* :upcase)
(defvar *print-length* nil)
(defvar *print-level* nil)
(defvar *print-gensym* t)
(defvar *print-array* t)
(defvar *print-readably* nil)
(defvar *print-circle* nil)
(defvar *print-pretty* nil)
(defvar *print-lines* nil)
(defvar *print-miser-width* nil)
(defvar *print-right-margin* nil)
(defvar *print-pprint-dispatch* nil)

;;; `(with-standard-io-syntax body...)` — BODY with every variable that
;;; controls reading and printing at its standard value (CLHS), so that what
;;; is printed inside reads back in any Lisp. `*readtable*` is a fresh copy of
;;; the standard readtable each time (decision 4): CLHS forbids modifying the
;;; standard one, and a copy makes that impossible rather than an error.
(defmacro with-standard-io-syntax (&body body)
  `(let ((*package* (find-package "COMMON-LISP-USER"))
         (*print-array* t)
         (*print-base* 10)
         (*print-case* :upcase)
         (*print-circle* nil)
         (*print-escape* t)
         (*print-gensym* t)
         (*print-length* nil)
         (*print-level* nil)
         (*print-lines* nil)
         (*print-miser-width* nil)
         (*print-pprint-dispatch* nil)
         (*print-pretty* nil)
         (*print-radix* nil)
         (*print-readably* t)
         (*print-right-margin* nil)
         (*read-base* 10)
         (*read-default-float-format* 'single-float)
         (*read-eval* t)
         (*read-suppress* nil)
         (*readtable* (copy-readtable nil)))
     ,@body))

;;; ------------------------------------------------------------------------
;;; The entry points

;;; Every way of printing comes here: `write-to-string` binds a variable for
;;; each keyword, then prints the object once into a string.
(defun write-to-string (object &key (escape *print-escape*) (base *print-base*)
                                    (radix *print-radix*) (case *print-case*)
                                    (length *print-length*) (level *print-level*)
                                    (gensym *print-gensym*) (array *print-array*)
                                    (readably *print-readably*) (circle *print-circle*)
                                    (pretty *print-pretty*) (lines *print-lines*)
                                    (miser-width *print-miser-width*)
                                    (right-margin *print-right-margin*)
                                    (pprint-dispatch *print-pprint-dispatch*))
  (let ((*print-escape* escape) (*print-base* base) (*print-radix* radix)
        (*print-case* case) (*print-length* length) (*print-level* level)
        (*print-gensym* gensym) (*print-array* array) (*print-readably* readably)
        (*print-circle* circle) (*print-pretty* pretty) (*print-lines* lines)
        (*print-miser-width* miser-width) (*print-right-margin* right-margin)
        (*print-pprint-dispatch* pprint-dispatch))
    (let ((out (make-string-output-stream)))
      (if *print-readably*
          ;; CLHS 22.1.3: printing readably implies escapes, arrays and
          ;; gensym prefixes, and nothing cut short by length or level.
          (let ((*print-escape* t) (*print-array* t) (*print-gensym* t)
                (*print-length* nil) (*print-level* nil) (*print-lines* nil))
            (%print-top object out))
          (%print-top object out))
      (get-output-stream-string out))))

;;; `princ` is `write` with neither escapes nor readability (CLHS 22.4).
(defun prin1-to-string (object) (write-to-string object :escape t))
(defun princ-to-string (object) (write-to-string object :escape nil :readably nil))

;;; Each builds the whole text first and writes it once, so printing to the
;;; console is one request to the host, not one per character. `write` takes
;;; every keyword `write-to-string` does, and `:stream`.
(defun write (object &rest keys &key stream escape base radix case length level gensym
                                     array readably circle pretty lines miser-width
                                     right-margin pprint-dispatch)
  (let ((keys (copy-list keys)))
    (remf keys :stream)
    (write-string (apply #'write-to-string object keys) stream))
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

;;; `(string x)`, the designator as a string, is in `strings.lisp` (#54).

;;; ------------------------------------------------------------------------
;;; Objects

;;; ------------------------------------------------------------------------
;;; Shared structure: `*print-circle*` (step Q, #64)
;;;
;;; With `*print-circle*` true, an object the printout would reach twice is
;;; written once, labelled — `#1=(a b)` — and referred to afterwards by its
;;; label, `#1#`. That is what lets a circular list be printed at all, and it
;;; is how the reader rebuilds the sharing (CLHS 22.1.3, 2.4.8.15).
;;;
;;; Two passes. The first walks the object as the printer would and records,
;;; in an `eq` hash table, every object it meets: `:once`, or `:many` when it
;;; meets it again, and it does not walk into an object a second time, which
;;; is why a cycle ends. The second pass prints, and the first time it comes
;;; to a `:many` object it gives it the next label. Which objects can be
;;; shared is SBCL's answer, measured: anything but a number, a character or
;;; a symbol with a home package — strings and gensyms included.
;;;
;;; The table needs keys compared by identity, which hash tables gained in
;;; this step for the purpose (decision 5): before it, Quokka's tables took
;;; only keys with contents to hash.

;;; The table of this printout, or nil when `*print-circle*` is false, and
;;; the last label given.
(defvar %circle-table nil)
(defvar %circle-label 0)

;;; The top of printing one object. Every printout binds its own table, so a
;;; `prin1-to-string` called while printing starts afresh.
(defun %print-top (object out)
  (let ((%circle-table (if *print-circle* (%find-shared object) nil))
        (%circle-label 0))
    (%print-object object out 0)))

(defun %shareable-p (object)
  (not (or (numberp object)
           (characterp object)
           (and (symbolp object) (symbol-package object)))))

;;; Does the printer write OBJECT's elements? Lists and vectors only: the
;;; structures the library makes are vectors underneath but print as `#<...>`.
;;; Arrays of several dimensions too (#67), whose elements are walked in
;;; row-major order; a vector's are its active ones, up to its fill pointer.
(defun %printed-as-vector-p (object)
  (and (arrayp object)
       (not (stringp object))
       (not (bit-vector-p object))
       *print-array*
       (not (%unreadable-structure-p object))))

(defun %unreadable-structure-p (object)
  (or (%package-p object) (%string-input-stream-p object) (%string-output-stream-p object)
      (%console-stream-p object) (%readtable-p object)))

(defun %find-shared (object)
  (let ((table (make-hash-table :test 'eq)))
    (%note-shared object table)
    table))

(defun %note-shared (object table)
  (if (%shareable-p object)
      (if (gethash object table)
          (setf (gethash object table) :many)
          (progn
            (setf (gethash object table) :once)
            (cond ((consp object)
                   (%note-shared (car object) table)
                   (%note-shared (cdr object) table))
                  ((and (%printed-as-vector-p object) (vectorp object))
                   (dotimes (i (length object))
                     (%note-shared (aref object i) table)))
                  ((%printed-as-vector-p object)
                   (dotimes (i (array-total-size object))
                     (%note-shared (row-major-aref object i) table))))))))

;;; Is OBJECT shared in this printout? Its mark: `:many` before its label is
;;; given, the label after.
(defun %circle-mark (object)
  (if (and %circle-table (%shareable-p object))
      (let ((mark (gethash object %circle-table)))
        (if (eq mark :once) nil mark))
      nil))

(defun %print-label (label suffix out)
  (write-char #\# out)
  (%print-integer-in-decimal label out)
  (write-char suffix out))

;;; Every object is printed through here: `#n#` if it has been labelled,
;;; `#n=` and the object if this is its first appearance and it is shared.
(defun %print-object (object out level)
  (let ((mark (%circle-mark object)))
    (cond ((null mark) (%print-plain object out level))
          ((integerp mark) (%print-label mark #\# out))
          (t (setq %circle-label (+ %circle-label 1))
             (setf (gethash object %circle-table) %circle-label)
             (%print-label %circle-label #\= out)
             (%print-plain object out level)))))

;;; The text for an object nothing can read back: `#<...>`, or, when printing
;;; readably, the error CLHS asks for instead.
(defun %print-unreadable (object text out)
  (if *print-readably*
      (error 'print-not-readable "this object cannot be printed readably" object)
      (write-string text out)))

(defun %print-plain (object out level)
  (cond ((integerp object) (%print-integer object out))
        ((floatp object) (%print-float object out))
        ((characterp object) (%print-character object out))
        ((symbolp object) (%print-symbol object out))
        ((stringp object) (%print-string object out))
        ((consp object) (%print-list object out level))
        ;; The library's own structures are vectors underneath (`defstruct`
        ;; makes vectors until there are structure types), but they are not
        ;; data anyone reads back, and SBCL writes them as `#<...>`.
        ((%package-p object)
         (%print-unreadable object
                            (if (package-name object)
                                (%string-append (%string-append "#<PACKAGE " (prin1-to-string (package-name object)))
                                                ">")
                                "#<PACKAGE (deleted)>")
                            out))
        ((%string-input-stream-p object) (%print-unreadable object "#<STRING-INPUT-STREAM>" out))
        ((%string-output-stream-p object) (%print-unreadable object "#<STRING-OUTPUT-STREAM>" out))
        ((%console-stream-p object) (%print-unreadable object "#<CONSOLE-STREAM>" out))
        ((%readtable-p object) (%print-unreadable object "#<READTABLE>" out))
        ;; Not an array — a function, a condition, a hash table: nothing
        ;; reads them back, so CLHS lets each implementation print them its
        ;; own way, and this is the Rust printer's, so both agree. Or
        ;; `*print-array*` false: an array by its dimensions alone.
        ((not (and (arrayp object) *print-array*))
         (if (arrayp object)
             (%print-unreadable object
                                (%string-append (%string-append "#<(ARRAY "
                                                                (princ-to-string (array-dimensions object)))
                                                ")>")
                                out)
             (%print-unreadable object (%printed object) out)))
        ;; `#*1011`: every bit, `*print-length*` or not, as SBCL prints one.
        ((bit-vector-p object)
         (write-string "#*" out)
         (dotimes (i (length object)) (write-char (if (eql (aref object i) 1) #\1 #\0) out)))
        ((vectorp object) (%print-vector object out level))
        (t (%print-array object out level))))

;;; `#2A((1 2) (3 4))`: the rank, then the elements as nested lists, one
;;; level of nesting per dimension, each counting against `*print-level*`
;;; and each list cut short by `*print-length*`. A rank-0 array is `#0A`
;;; and its one element.
(defun %print-array (array out level)
  (let ((dimensions (array-dimensions array)))
    (write-char #\# out)
    (%print-integer-in-decimal (length dimensions) out)
    (write-char #\A out)
    (if (null dimensions)
        (%print-object (aref array) out level)
        (%print-array-slice array dimensions 0 out level))))

(defun %print-array-slice (array dimensions start out level)
  (if (%too-deep-p level)
      (write-char #\# out)
      (let ((stride (%product (cdr dimensions))))
        (write-char #\( out)
        (dotimes (i (car dimensions))
          (if (> i 0) (write-char #\Space out))
          (if (%too-long-p i)
              (progn (write-string "..." out) (return))
              (if (cdr dimensions)
                  (%print-array-slice array (cdr dimensions) (+ start (* i stride)) out (+ level 1))
                  (%print-object (row-major-aref array (+ start i)) out (+ level 1)))))
        (write-char #\) out))))

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
                ;; A shared tail is printed after a dot, so that its label
                ;; stands for the rest of the list: `(a . #1#)`.
                ((and (consp rest) (null (%circle-mark rest)))
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

;;; A float as SBCL prints one (step M, #17): the shortest digits that read
;;; back as the same double (Rust finds them, decision 5), between 10^-3 and
;;; 10^7 in positional notation — 0.001, 123.5, 9999999.0 — and outside it
;;; with an exponent — 1.0e7, 9.99e-4. Always in decimal, whatever
;;; `*print-base*` says. No exponent marker says which format, because the
;;; double is the only one and so always `*read-default-float-format*`.
(defun %print-float (float out)
  (if (%negative-float-p float) (write-char #\- out))
  (if (zerop float)
      (write-string "0.0" out)
      (multiple-value-bind (digits exponent) (%float-digits float)
        (if (and (>= exponent -3) (< exponent 7))
            (write-string (%fixed-shortest float) out)
            (progn
              (write-string (subseq digits 0 1) out)
              (write-char #\. out)
              (write-string (if (> (length digits) 1) (subseq digits 1) "0") out)
              (write-char #\e out)
              (%print-integer-in-decimal exponent out))))))

(defun %print-integer-in-decimal (integer out)
  (if (< integer 0) (write-char #\- out))
  (%print-digits (%integer-digits integer 10 nil) out))

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
        (%token-integer name 10)
        (%ratio-or-float-p name)
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
