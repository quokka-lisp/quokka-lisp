;;;; The reader, in Lisp (#45, step C of #39).
;;;;
;;;; Text in, objects out — what `reader.rs` does, written again in Lisp so
;;;; that it can intern through `*package*` and read `pkg:sym` and `pkg::sym`,
;;;; which the Rust reader cannot (#43). CLHS chapter 2 is the specification,
;;;; and 2.2, *Reader Algorithm*, is the loop at the heart of `%read` below.
;;;;
;;;; Decided in #45 (Carsten):
;;;;
;;;; - It reads from **string input streams**, a Lisp structure with
;;;;   `read-char`, `peek-char` and `unread-char`. Output streams come with the
;;;;   printer (step G). That settles #42 for input.
;;;; - It is driven by a **readtable**: `(`, `'`, `"`, `;`, backquote and
;;;;   comma are *reader macros* — ordinary Lisp functions found in a table by
;;;;   character — and `#` is a *dispatching* macro character with its own
;;;;   table. A program can add its own with `set-macro-character`.
;;;;
;;;; Backquote expands exactly as `reader.rs::quasiquote` expands it, into
;;;; `cons`, `append` and `quote`. Step F compiles the compiler's source with
;;;; this reader and compares the result with what the Rust reader's source
;;;; compiled to; a different expansion would change the bytes without
;;;; anything being wrong.
;;;;
;;;; This file comes before `packages.lisp` in the library, because that file
;;;; must be the last the Rust reader reads (see its header). The package
;;;; functions called here are looked up when called, not when compiled, so
;;;; the order costs nothing.

;;; `*package*` gets its value in `packages.lisp`, the library's last file,
;;; once there are packages to be in. It is proclaimed special here, before
;;; anything binds it: a `let` of a variable not yet special compiles to an
;;; ordinary local, and `load`'s `(let ((*package* *package*)) ...)` did
;;; exactly that, so an `in-package` inside a loaded file changed the global
;;; package and leaked out of it (#47).
(defvar *package*)

;;; `#+`, `#-` and `#.` (#62, part 4). `*features*` says what this Lisp is:
;;; not `:ansi-cl`, since Quokka is a subset that does not yet conform, and
;;; nothing about the machine, since an image is the same on every target.
;;; `*read-suppress*`, when true, makes the reader read text for its shape
;;; only — no symbols interned, no numbers made, no errors for syntax it does
;;; not know — which is how `#-sbcl` skips a form written for another Lisp.
(defvar *features* '(:quokka :common-lisp :ieee-floating-point))
(defvar *read-suppress* nil)
(defvar *read-eval* t)

;;; The radix integers are read in (step Q, #64).
(defvar *read-base* 10)

;;; ------------------------------------------------------------------------
;;; Characters
;;;
;;; `char-code`, `code-char`, `char-upcase`, `char-downcase` and
;;; `alpha-char-p` are primitives, because the case and letter tables are
;;; Unicode's and Rust's standard library already carries them. The rest
;;; follow from those.

;;; Upper case means having a lower-case partner, and the other way round.
(defun upper-case-p (char) (not (eq (char-downcase char) char)))
(defun lower-case-p (char) (not (eq (char-upcase char) char)))
(defun both-case-p (char) (or (upper-case-p char) (lower-case-p char)))

;;; The weight of CHAR as a digit in RADIX, or nil. Letters are digits from
;;; ten upwards, so `#\z` is 35 in radix 36.
(defun digit-char-p (char &optional (radix 10))
  (let* ((code (char-code char))
         (weight (cond ((and (> code 47) (< code 58)) (- code 48))
                       ((and (> code 64) (< code 91)) (- code 55))
                       ((and (> code 96) (< code 123)) (- code 87))
                       (t radix))))
    (if (< weight radix) weight nil)))

(defun alphanumericp (char)
  (or (alpha-char-p char) (if (digit-char-p char) t nil)))

;;; Character names (step W, #70). Every Latin-1 character has the name
;;; SBCL gives it, from the table in `char-names.lisp`; above 255 there are
;;; none. `name-char` ignores case, as CLHS requires, and also knows SBCL's
;;; other names for some controls, and `U+E9` or `UE9` — `U`, an optional
;;; `+`, and the code in hexadecimal — for any character at all.
;;; One more than the largest code a character can have: Unicode's range, as
;;; in SBCL. `code-char` answers nil for a surrogate half inside it.
(defconstant char-code-limit 1114112)

(defun char-name (char)
  (let ((code (char-code char)))
    (if (< code 256) (aref %latin-1-char-names code) nil)))

(defvar %character-aliases
  '(("Linefeed" . 10) ("Null" . 0) ("Delete" . 127) ("Del" . 127) ("Escape" . 27)
    ("Lf" . 10) ("Cr" . 13) ("Bs" . 8) ("Ht" . 9) ("Ff" . 12) ("Nl" . 10) ("Sp" . 32)))

(defun name-char (name)
  (let ((name (string name)))
    (or (let ((code (position name %latin-1-char-names :test #'string-equal)))
          (and code (code-char code)))
        (let ((alias (assoc name %character-aliases :test #'string-equal)))
          (and alias (code-char (cdr alias))))
        (%hex-char-name name))))

(defun %hex-char-name (name)
  (let* ((end (length name))
         (start (if (and (> end 1) (eql (char name 1) #\+)) 2 1)))
    (if (and (< start end) (char-equal (char name 0) #\U))
        (let ((code (parse-integer name :start start :radix 16 :junk-allowed t)))
          ;; `parse-integer` with junk allowed stops at the first non-digit;
          ;; a name is only a code if every character after the `U` is one.
          (if (and code (every (lambda (c) (digit-char-p c 16)) (subseq name start))
                   (< code char-code-limit))
              (code-char code)
              nil))
        nil)))

;;; ------------------------------------------------------------------------
;;; String input streams
;;;
;;; A stream is a source or sink of characters that remembers where it is.
;;; This is the only kind tier 1 has for input: a string and a position.
;;; Tier 2 makes streams generic functions over many kinds (CLOS); the
;;; functions here keep their standard names and arguments so that callers
;;; do not change when that happens.

(defstruct %string-input-stream string position end)

(defun make-string-input-stream (string &optional (start 0) end)
  (make-%string-input-stream :string string :position start
                             :end (if end end (length string))))

;;; Output streams are in `printer.lisp`; this answers for them too.
(defun streamp (object)
  (or (%string-input-stream-p object) (output-stream-p object)))

;;; There is no console to read from yet, so standard input is an empty
;;; string: reading it meets end of file at once.
(defvar *standard-input* (make-string-input-stream ""))

;;; A stream designator: nil and t both mean standard input.
(defun %input-stream (designator)
  (cond ((or (null designator) (eq designator t)) *standard-input*)
        ((%string-input-stream-p designator) designator)
        (t (error "not an input stream"))))

;;; Running out of input is an `end-of-file` condition (#70), so that a
;;; program can tell it from a syntax error. In CLHS it is also a
;;; `stream-error`; until conditions have a hierarchy (tier 2), a handler
;;; must name `end-of-file` itself to catch it.
(defun %end-of-file (eof-error-p eof-value)
  (if eof-error-p (error 'end-of-file "end of file") eof-value))

(defun read-char (&optional stream (eof-error-p t) eof-value recursive-p)
  (let* ((stream (%input-stream stream))
         (at (%string-input-stream-position stream)))
    (if (< at (%string-input-stream-end stream))
        (progn (setf (%string-input-stream-position stream) (+ at 1))
               (aref (%string-input-stream-string stream) at))
        (%end-of-file eof-error-p eof-value))))

;;; The reader's own `read-char`: the next character of STREAM, a string
;;; input stream, or nil at the end. `read` resolves its stream designator
;;; once, so the loops that run for every character of the text — tokens,
;;; whitespace, strings and comments — need not do it again, nor parse
;;; `read-char`'s four optional arguments; that was a good part of what a
;;; character cost (#70, chapter 58). Tier 2's generic streams will make this
;;; a generic function too.
(defun %next-char (stream)
  (let ((at (%string-input-stream-position stream)))
    (if (< at (%string-input-stream-end stream))
        (progn (setf (%string-input-stream-position stream) (+ at 1))
               (aref (%string-input-stream-string stream) at))
        nil)))

;;; Put back the character just read. CLHS allows only that one, which is all
;;; a string stream needs to make it true: step back one.
(defun unread-char (char &optional stream)
  (let ((stream (%input-stream stream)))
    (setf (%string-input-stream-position stream)
          (- (%string-input-stream-position stream) 1))
    nil))

;;; The next character without consuming it. PEEK-TYPE nil looks at the next
;;; character; t skips whitespace first; a character skips up to that one.
(defun peek-char (&optional peek-type stream (eof-error-p t) eof-value recursive-p)
  (let* ((stream (%input-stream stream))
         (char (read-char stream nil nil)))
    (cond ((null char) (%end-of-file eof-error-p eof-value))
          ((or (and (eq peek-type t) (%whitespacep char))
               (and (characterp peek-type) (not (eq peek-type char))))
           (peek-char peek-type stream eof-error-p eof-value))
          (t (unread-char char stream) char))))

;;; `(with-input-from-string (var string :start s :end e) body...)`. CLHS's
;;; `:index`, which stores the final position in a place, is not here yet.
;;; The options are parsed by a function with an ordinary `&key` list, because
;;; a nested macro pattern cannot have one here.
(defmacro with-input-from-string (binding &body body)
  (let ((bounds (apply #'%string-stream-bounds (cddr binding))))
    `(let ((,(car binding) (make-string-input-stream ,(cadr binding) ,@bounds)))
       ,@body)))

(defun %string-stream-bounds (&key (start 0) end) (list start end))

;;; ------------------------------------------------------------------------
;;; Readtables
;;;
;;; A readtable says what each character means to the reader. `macros` maps
;;; a macro character to `(function . non-terminating-p)`; `dispatch` maps a
;;; dispatching macro character, such as `#`, to a second table from the
;;; character after it to a function. Both are `eql` hash tables, keyed by
;;; characters.
;;;
;;; Every other character has a *syntax type* (CLHS 2.1.4): whitespace,
;;; constituent — an ordinary character of a token — single escape `\`, or
;;; multiple escape `|`. Those of the standard syntax are fixed in
;;; `%standard-syntax`; `syntax` holds only the characters whose type a
;;; program has changed with `set-syntax-from-char` (step W, #70), and is nil
;;; until it does. While it is nil the reader takes the standard types
;;; without looking anything up, so reading in the standard syntax — every
;;; library file, every rebuild — pays nothing for the table.
;;;
;;; `case` is the readtable's case mode (step W, #70): what the reader does
;;; to the unescaped letters of a token. `:upcase`, the standard, raises them;
;;; `:downcase` lowers them; `:preserve` leaves them; and `:invert` flips them
;;; when they are all of one case and leaves a mixed token alone, so that
;;; `zebra` reads as `ZEBRA` and `Zebra` as `Zebra` (CLHS 23.1.2). The printer
;;; reads the same slot, to write names that read back.

(defstruct %readtable macros dispatch (case :upcase) syntax)

;;; `clrhash` is standard and was missing; `copy-readtable` is its first
;;; caller. `maphash` walks a list of the entries taken before it starts, so
;;; removing them as it goes is safe.
(defun clrhash (table)
  (maphash (lambda (key value) (remhash key table)) table)
  table)

(defun readtablep (object) (%readtable-p object))

(defun %check-readtable (object)
  (if (%readtable-p object) object (error 'type-error "not a readtable" object)))

(defun readtable-case (readtable) (%readtable-case (%check-readtable readtable)))

(defun %set-readtable-case (readtable mode)
  (if (not (member mode '(:upcase :downcase :preserve :invert)))
      (error 'type-error "a readtable case is :upcase, :downcase, :preserve or :invert" mode))
  (setf (%readtable-case (%check-readtable readtable)) mode))

(defsetf readtable-case %set-readtable-case)

;;; Filled in at the end of this file, once its functions exist.
(defvar %standard-readtable
  (make-%readtable :macros (make-hash-table) :dispatch (make-hash-table)))

;;; A readtable designator: nil means the standard readtable.
(defun %the-readtable (designator)
  (if designator designator %standard-readtable))

;;; A copy of FROM, into TO if there is one. The standard readtable itself is
;;; never changed after this file; programs change copies, and `*readtable*`
;;; starts as one.
(defun copy-readtable (&optional (from *readtable*) to)
  (let ((from (%the-readtable from))
        (to (if to to (make-%readtable :macros (make-hash-table)
                                       :dispatch (make-hash-table)))))
    (clrhash (%readtable-macros to))
    (clrhash (%readtable-dispatch to))
    (setf (%readtable-case to) (%readtable-case from))
    (setf (%readtable-syntax to) (if (%readtable-syntax from) (%copy-table (%readtable-syntax from))))
    (maphash (lambda (char entry) (setf (gethash char (%readtable-macros to)) entry))
             (%readtable-macros from))
    (maphash (lambda (char table)
               (let ((copy (make-hash-table)))
                 (maphash (lambda (sub function) (setf (gethash sub copy) function)) table)
                 (setf (gethash char (%readtable-dispatch to)) copy)))
             (%readtable-dispatch from))
    to))

(defun %copy-table (table)
  (let ((copy (make-hash-table)))
    (maphash (lambda (key value) (setf (gethash key copy) value)) table)
    copy))

;;; A macro character is looked up before the syntax table, so a function on
;;; any character wins over its type. The one place the reader does not look
;;; is the fast path, which knows whitespace and the escapes by their codes:
;;; a macro on one of those turns the syntax table on, so that the reader
;;; looks.
(defun set-macro-character (char function &optional non-terminating-p (readtable *readtable*))
  (let ((readtable (%the-readtable readtable)))
    (setf (gethash char (%readtable-macros readtable)) (cons function non-terminating-p))
    (if (member (%standard-syntax char) '(:whitespace :single-escape :multiple-escape))
        (%syntax-table readtable))
    t))

;;; The syntax table of READTABLE, made on first use.
(defun %syntax-table (readtable)
  (or (%readtable-syntax readtable)
      (setf (%readtable-syntax readtable) (make-hash-table))))

;;; `(set-syntax-from-char to-char from-char to-readtable from-readtable)`:
;;; make TO-CHAR in TO-READTABLE what FROM-CHAR is in FROM-READTABLE (by
;;; default the standard one). A macro character brings its function, and a
;;; dispatching one a copy of its whole dispatch table (CLHS); any other
;;; character brings its syntax type, and TO-CHAR stops being a macro
;;; character. A constituent's *traits* — whether it is a letter, a digit, a
;;; package marker, or invalid — belong to the character and do not move:
;;; `!` made a constituent is an ordinary letter, and a space made one is
;;; still invalid in a token.
(defun set-syntax-from-char (to-char from-char &optional (to-readtable *readtable*) from-readtable)
  (let* ((to (%the-readtable to-readtable))
         (from (%the-readtable from-readtable))
         (entry (gethash from-char (%readtable-macros from)))
         (dispatch (gethash from-char (%readtable-dispatch from)))
         (syntax (%syntax-table to)))
    (remhash to-char (%readtable-dispatch to))
    (cond (entry
           (setf (gethash to-char (%readtable-macros to)) (cons (car entry) (cdr entry)))
           (if dispatch (setf (gethash to-char (%readtable-dispatch to)) (%copy-table dispatch)))
           (remhash to-char syntax))
          (t (remhash to-char (%readtable-macros to))
             (let ((type (%table-syntax from from-char)))
               (setf (gethash to-char syntax) (if (eq type :invalid) :constituent type)))))
    t))

;;; The function and whether it is non-terminating, or nil.
(defun get-macro-character (char &optional (readtable *readtable*))
  (let ((entry (gethash char (%readtable-macros (%the-readtable readtable)))))
    (if entry (values (car entry) (cdr entry)) (values nil nil))))

(defun make-dispatch-macro-character (char &optional non-terminating-p (readtable *readtable*))
  (let ((readtable (%the-readtable readtable)))
    (setf (gethash char (%readtable-dispatch readtable)) (make-hash-table))
    (set-macro-character char #'%read-dispatch non-terminating-p readtable)))

;;; Sub-characters are case-insensitive: `#c` and `#C` are one.
(defun set-dispatch-macro-character (char sub-char function &optional (readtable *readtable*))
  (setf (gethash (char-upcase sub-char) (%dispatch-table char readtable)) function)
  t)

(defun get-dispatch-macro-character (char sub-char &optional (readtable *readtable*))
  (values (gethash (char-upcase sub-char) (%dispatch-table char readtable))))

(defun %dispatch-table (char readtable)
  (let ((table (gethash char (%readtable-dispatch (%the-readtable readtable)))))
    (if table table (error "not a dispatching macro character"))))

;;; ------------------------------------------------------------------------
;;; The reader algorithm (CLHS 2.2)

;;; Whitespace in the standard syntax: space, newline, tab, return, page.
(defun %standard-whitespace-p (char)
  (let ((code (char-code char)))
    (or (eq code 32) (eq code 10) (eq code 9) (eq code 13) (eq code 12))))

;;; The standard syntax type of CHAR. `:invalid` is a constituent with the
;;; *invalid* trait (CLHS 2.1.4.2): backspace and rubout, which may not
;;; appear unescaped in a token. Folding the trait into the type lets the
;;; token loop find it with the test it makes anyway, rather than a second
;;; one on every character. The order puts the common case, an ordinary
;;; printing character, after as few tests as possible: the reader asks this
;;; of every character of every token it reads.
(defun %standard-syntax (char)
  (let ((code (char-code char)))
    (cond ((< code 33)
           (cond ((or (eq code 32) (eq code 10) (eq code 9) (eq code 13) (eq code 12)) :whitespace)
                 ((eq code 8) :invalid)
                 (t :constituent)))
          ((eq code 92) :single-escape)
          ((eq code 124) :multiple-escape)
          ((eq code 127) :invalid)
          (t :constituent))))

;;; The syntax type of CHAR in READTABLE, setting macro characters aside. A
;;; character made a constituent keeps its invalid trait: a space made one
;;; is `:invalid`.
(defun %table-syntax (readtable char)
  (let* ((syntax (%readtable-syntax readtable))
         (type (if syntax (gethash char syntax))))
    (cond ((null type) (%standard-syntax char))
          ((and (eq type :constituent) (%invalid-constituent-p char)) :invalid)
          (t type))))

;;; What CHAR is to the reader now: `:terminating` or `:non-terminating` for
;;; a macro character, otherwise its syntax type. Every question the reader
;;; asks about a character is a question about this.
(defun %char-syntax (char)
  (let* ((readtable *readtable*)
         (entry (gethash char (%readtable-macros readtable))))
    (cond (entry (if (cdr entry) :non-terminating :terminating))
          ((%readtable-syntax readtable) (%table-syntax readtable char))
          (t (%standard-syntax char)))))

;;; Whitespace in the current readtable. The fast path first: with no
;;; syntax table, whitespace is the standard five.
(defun %whitespacep (char)
  (if (%readtable-syntax *readtable*)
      (eq (%char-syntax char) :whitespace)
      (%standard-whitespace-p char)))

;;; Does CHAR end a token? Whitespace does, and so does a terminating macro
;;; character — `)` ends `foo` in `(foo)`, where `#` would not.
(defun %terminatorp (char)
  (let ((syntax (%char-syntax char)))
    (or (eq syntax :whitespace) (eq syntax :terminating))))

;;; The characters a token may not contain unescaped, whatever their syntax
;;; type: the standard whitespace, backspace and rubout. CLHS 2.1.4.2 calls
;;; this the *invalid* trait; SBCL signals `reader-error`, so a stray
;;; control character in source is found where it is rather than becoming
;;; part of a name.
(defun %invalid-constituent-p (char)
  (let ((code (char-code char)))
    (and (or (< code 33) (eq code 127))
         (or (%standard-whitespace-p char) (eq code 8) (eq code 127)))))

;;; The next character that is not whitespace, or nil at the end. Source
;;; text is largely indentation, so the readtable is asked once whether it
;;; has a syntax table, not once for every space; without one, the loop
;;; tests the codes itself.
(defun %skip-whitespace (stream)
  (if (%readtable-syntax *readtable*)
      (%skip-whitespace-in-table stream)
      (%skip-standard-whitespace stream)))

(defun %skip-standard-whitespace (stream)
  (do ((char (%next-char stream) (%next-char stream)))
      ((not (and char (%standard-whitespace-p char))) char)))

(defun %skip-whitespace-in-table (stream)
  (do ((char (%next-char stream) (%next-char stream)))
      ((not (and char (eq (%char-syntax char) :whitespace))) char)))

;;; `(read stream eof-error-p eof-value recursive-p)` — the next object.
;;;
;;; Skip whitespace; at a macro character, call its function, which may
;;; return an object or nothing — a comment returns nothing, and reading
;;; carries on after it; at anything else, read a token. End of input before
;;; anything is found is the end of file; in the middle of an object it is
;;; always an error, which `%read-required` arranges.
;;;
;;; A top-level read — not one a reader macro makes in the middle of an
;;; object — also consumes one whitespace character after the object, as
;;; SBCL's does: `(read-from-string "a b")` stops at 2, not 1.
(defun read (&optional stream (eof-error-p t) eof-value recursive-p)
  (let* ((stream (%input-stream stream))
         (object (%read stream eof-error-p eof-value)))
    (unless recursive-p
      (let ((next (read-char stream nil nil)))
        (if (and next (not (%whitespacep next))) (unread-char next stream))))
    object))

(defun %read (stream eof-error-p eof-value)
  (let ((char (%skip-whitespace stream)))
    (if (null char)
        (%end-of-file eof-error-p eof-value)
        (let ((found (%read-maybe-nothing stream char)))
          (if found (car found) (%read stream eof-error-p eof-value))))))

;;; An object that must be there: the thing after `'`, or a list element.
(defun %read-required (stream) (%read stream t nil))

;;; Read starting at CHAR, already consumed. A list of the object, or nil if
;;; CHAR began something that reads as nothing.
(defun %read-maybe-nothing (stream char)
  (let ((entry (gethash char (%readtable-macros (%the-readtable *readtable*)))))
    (if entry
        (multiple-value-list (funcall (car entry) stream char))
        (list (%read-token stream char)))))

;;; `read` without that last step (step W, #70): the whitespace that ended a
;;; token stays in the stream. A program that reads a token and then looks at
;;; what follows it — whether a newline came next, say — needs it there. A
;;; read made inside another (RECURSIVE-P) never consumes the delimiter
;;; anyway, so both kinds are the same call.
(defun read-preserving-whitespace (&optional stream (eof-error-p t) eof-value recursive-p)
  (%read (%input-stream stream) eof-error-p eof-value))

;;; `(read-from-string string eof-error-p eof-value :start :end
;;; :preserve-whitespace)` — the object, and the position after it.
(defun read-from-string (string &optional (eof-error-p t) eof-value
                         &key (start 0) end preserve-whitespace)
  (let* ((stream (make-string-input-stream string start end))
         (object (if preserve-whitespace
                     (read-preserving-whitespace stream eof-error-p eof-value)
                     (read stream eof-error-p eof-value))))
    (values object (%string-input-stream-position stream))))

;;; ------------------------------------------------------------------------
;;; Tokens (CLHS 2.3)
;;;
;;; A token is a run of characters up to whitespace or a terminating macro
;;; character. Unescaped letters are folded by the readtable's case as they
;;; are collected — upcased, in the standard case. `\` escapes one character
;;; and `|...|` a run of them: an escaped character keeps its case, is never a
;;; package marker, and makes the token a symbol even if it looks like a
;;; number.
;;;
;;; `:invert` cannot fold as it goes, since whether a letter flips depends on
;;; every other unescaped letter in the token. It collects the letters as
;;; they are, with the positions of the escaped characters in ESCAPES, and
;;; `%invert-token` decides once the token is complete. Only `:invert` needs
;;; those positions, but they are kept in every mode: a token rarely has an
;;; escape, so the list is nearly always empty and costs nothing.

(defun %read-token (stream char)
  (multiple-value-bind (text escaped colons) (%start-token stream char)
    (if *read-suppress* nil (%interpret-token text escaped colons))))

(defun %start-token (stream char)
  (%collect-token stream char (%readtable-case (%the-readtable *readtable*))))

;;; The token's text, whether any of it was escaped, and where its unescaped
;;; colons are. CHARS is the token so far, backwards; COLONS the positions of
;;; unescaped colons, and ESCAPES those of escaped characters, both also
;;; backwards. The terminator is put back for the reader to see next. A
;;; non-terminating macro character inside a token is an ordinary character
;;; of it, as `#` is in `a#b`. Inside `|...|` everything is literal up to the
;;; closing bar, except that `\` still escapes the next character — or
;;; whichever characters have those syntax types now.
;;;
;;; A loop over local variables, not a function calling itself once a
;;; character: this runs for every character the reader reads, and a call
;;; with all of the token's state as arguments cost more than anything done
;;; with the character. Step W found that the hard way (chapter 58).
(defun %collect-token (stream char case)
  (let ((chars nil) (count 0) (escaped nil) (colons nil) (escapes nil))
    (do () ((null char))
      (let ((syntax (%char-syntax char)))
        (cond ((or (eq syntax :constituent) (eq syntax :non-terminating))
               (if (eq char #\:) (setq colons (cons count colons)))
               (setq chars (cons (cond ((eq case :upcase) (char-upcase char))
                                       ((eq case :downcase) (char-downcase char))
                                       (t char))
                                 chars)
                     count (+ count 1)
                     char (%next-char stream)))
              ((eq syntax :single-escape)
               (setq escapes (cons count escapes)
                     chars (cons (read-char stream) chars)
                     count (+ count 1)
                     escaped t
                     char (%next-char stream)))
              ((eq syntax :multiple-escape)
               (setq escaped t)
               (do ((next (read-char stream) (read-char stream)))
                   ((eq (%char-syntax next) :multiple-escape))
                 (if (eq (%char-syntax next) :single-escape) (setq next (read-char stream)))
                 (setq escapes (cons count escapes)
                       chars (cons next chars)
                       count (+ count 1)))
               (setq char (%next-char stream)))
              ((or (eq syntax :whitespace) (eq syntax :terminating))
               (unread-char char stream)
               (setq char nil))
              (t (error 'reader-error "a character that cannot be in a token" char)))))
    (%finish-token chars count escaped colons escapes case)))

(defun %finish-token (chars count escaped colons escapes case)
  (let ((text (%chars-string chars count)))
    (if (eq case :invert) (%invert-token text escapes))
    (values text escaped colons)))

;;; `:invert`: if the unescaped letters of TEXT are all upper case, lower
;;; them; if all lower case, raise them; if mixed, leave them (CLHS 23.1.2).
(defun %invert-token (text escapes)
  (let ((upper nil) (lower nil))
    (dotimes (i (length text))
      (unless (member i escapes)
        (let ((char (aref text i)))
          (if (upper-case-p char) (setq upper t))
          (if (lower-case-p char) (setq lower t)))))
    (unless (and upper lower)
      (dotimes (i (length text))
        (unless (member i escapes)
          (let ((char (aref text i)))
            (setf (aref text i) (if upper (char-downcase char) (char-upcase char)))))))
    text))

;;; A string of COUNT characters from CHARS, which is backwards.
(defun %chars-string (chars count)
  (let ((string (make-string count)))
    (%fill-backwards string chars (- count 1))))

(defun %fill-backwards (string chars at)
  (if (null chars)
      string
      (progn (setf (aref string at) (car chars))
             (%fill-backwards string (cdr chars) (- at 1)))))

(defun %substring (string start end)
  (let ((copy (make-string (- end start))))
    (%copy-chars string start end copy 0)))

(defun %copy-chars (from start end to at)
  (if (< start end)
      (progn (setf (aref to at) (aref from start))
             (%copy-chars from (+ start 1) end to (+ at 1)))
      to))

;;; A number, or a symbol found or made through the package system.
(defun %interpret-token (text escaped colons)
  (let ((integer (if (or escaped colons) nil (%token-integer text))))
    (cond (integer integer)
          ((and (not escaped) (not colons) (%ratio-or-float-p text))
           (if (%ratio-token-p text)
               (error "Quokka reads no ratios until they exist (#59)")
               (%read-float text)))
          ((and (not escaped) (%all-dots-p text 0 (length text)))
           (error "a token of only dots"))
          ((null colons) (values (intern text *package*)))
          (t (%read-qualified text colons)))))

;;; `:name` is a keyword, and so is `::name`, as SBCL reads it; `pkg:name`
;;; must be external in PKG; `pkg::name` is interned there. Anything else with
;;; a colon in it is an error.
(defun %read-qualified (text colons)
  (let ((end (length text)))
    (cond ((and (null (cdr colons)) (eq (car colons) 0))
           (values (intern (%substring text 1 end) "KEYWORD")))
          ((and (null (cddr colons)) (eq (car colons) 1) (eq (cadr colons) 0))
           (values (intern (%substring text 2 end) "KEYWORD")))
          ((null (cdr colons))
           (%external-symbol (%reader-package text (car colons))
                             (%substring text (+ (car colons) 1) end)))
          ((and (null (cddr colons))
                (eq (car colons) (+ (cadr colons) 1))
                (> (cadr colons) 0))
           (values (intern (%substring text (+ (car colons) 1) end)
                           (%reader-package text (cadr colons)))))
          (t (error "too many package markers in a symbol")))))

(defun %reader-package (text colon)
  (let ((package (find-package (%substring text 0 colon))))
    (if package package (error "no package by that name"))))

(defun %external-symbol (package name)
  (multiple-value-bind (symbol status) (find-symbol name package)
    (if (eq status :external)
        symbol
        (error "that symbol is not external in its package"))))

;;; An integer: a sign, digits in `*read-base*`, and an optional trailing
;;; point, which makes the digits decimal whatever the base — `12.` is twelve
;;; even in base 16, where `ff` is 255 and `1e5` an integer, not a float
;;; (CLHS 2.3.1, step Q). The printer and `floats.lisp` pass 10, since they
;;; ask about decimal text.
(defun %token-integer (text &optional (base *read-base*))
  (let* ((length (length text))
         (start (%skip-sign text 0 length))
         (point (and (> length (+ start 1)) (eq (aref text (- length 1)) #\.)))
         (end (if point (- length 1) length))
         (radix (if point 10 base)))
    (if (and (< start end) (eq (%skip-digits text start end radix) end))
        (let ((value (%digits-value text start end 0 radix)))
          (if (eq (aref text 0) #\-) (- value) value))
        nil)))

(defun %skip-sign (text at end)
  (if (and (< at end) (or (eq (aref text at) #\+) (eq (aref text at) #\-))) (+ at 1) at))

(defun %skip-digits (text at end &optional (radix 10))
  (if (and (< at end) (digit-char-p (aref text at) radix))
      (%skip-digits text (+ at 1) end radix)
      at))

(defun %digits-value (text at end value &optional (radix 10))
  (if (< at end)
      (%digits-value text (+ at 1) end (+ (* value radix) (digit-char-p (aref text at) radix))
                     radix)
      value))

;;; One or more digits, and nothing after them.
(defun %digits-to-end-p (text at end)
  (and (< at end) (eq (%skip-digits text at end) end)))

(defun %all-dots-p (text at end)
  (if (< at end) (and (eq (aref text at) #\.) (%all-dots-p text (+ at 1) end)) t))

;;; Does TEXT have the syntax of a ratio (`1/2`) or a float (`1.5`, `.5`,
;;; `1e5`, `1.5d0`)? CLHS 2.3.1. Floats are read (step M, #17); ratios do not
;;; exist yet (#59), and reading `1/2` as a symbol would succeed quietly and
;;; mean something else once they do, so the reader refuses it instead.
(defun %ratio-or-float-p (text)
  (let* ((end (length text))
         (start (%skip-sign text 0 end))
         (int-end (%skip-digits text start end)))
    (if (and (< int-end end) (eq (aref text int-end) #\/))
        (and (> int-end start) (%digits-to-end-p text (+ int-end 1) end))
        (let* ((point (and (< int-end end) (eq (aref text int-end) #\.)))
               (fraction-end (if point (%skip-digits text (+ int-end 1) end) int-end))
               (fraction (and point (> fraction-end (+ int-end 1)))))
          (cond ((eq fraction-end end) fraction)
                ((and (or fraction (> int-end start))
                      (member (char-upcase (aref text fraction-end)) '(#\E #\S #\F #\D #\L)))
                 (%digits-to-end-p text (%skip-sign text (+ fraction-end 1) end) end))
                (t nil))))))

;;; A ratio is digits, a slash, digits; any other token `%ratio-or-float-p`
;;; accepts is a float.
(defun %ratio-token-p (text)
  (let ((end (length text)))
    (%position-of-char #\/ text 0 end)))

(defun %position-of-char (char text at end)
  (cond ((eq at end) nil)
        ((eq (aref text at) char) at)
        (t (%position-of-char char text (+ at 1) end))))

;;; A float (step M, #17). The token's syntax was checked above; Rust turns
;;; the digits into the nearest double (decision 5), once they are in its
;;; syntax: CL's exponent markers `e s f d l` all become `e`, since every
;;; float format is the double (decision 2), and a point with no digits after
;;; it gets a zero, so that `1.e5` is `1.0e5`.
(defun %read-float (text)
  (let ((out (make-string-output-stream))
        (end (length text)))
    (dotimes (i end)
      (let ((char (aref text i)))
        (cond ((member (char-upcase char) '(#\E #\S #\F #\D #\L)) (write-char #\e out))
              (t (write-char char out)))
        (if (and (eq char #\.)
                 (or (eq (+ i 1) end) (not (digit-char-p (aref text (+ i 1))))))
            (write-char #\0 out))))
    (%parse-double (get-output-stream-string out))))

;;; `(parse-integer string &key start end radix junk-allowed)` (#68): an
;;; integer, with whitespace around it allowed and a sign before it, and the
;;; index where reading stopped. Anything else in the range is an error of
;;; type `parse-error`, unless JUNK-ALLOWED, when reading stops at it and the
;;; integer is nil if there were no digits.
(defun parse-integer (string &key (start 0) end (radix 10) junk-allowed)
  (multiple-value-bind (start end) (%bounds string start end)
    (let ((i start) (sign 1) (value 0) (digits nil))
      (do () ((not (and (< i end) (%standard-whitespace-p (char string i)))))
        (setq i (+ i 1)))
      (if (and (< i end) (member (char string i) '(#\+ #\-)))
          (progn (if (eql (char string i) #\-) (setq sign -1))
                 (setq i (+ i 1))))
      (do () ((not (and (< i end) (digit-char-p (char string i) radix))))
        (setq value (+ (* value radix) (digit-char-p (char string i) radix))
              digits t
              i (+ i 1)))
      (if junk-allowed
          (values (if digits (* sign value) nil) i)
          (progn
            (do () ((not (and (< i end) (%standard-whitespace-p (char string i)))))
              (setq i (+ i 1)))
            (if (or (not digits) (< i end))
                (error 'parse-error "not an integer" (subseq string start end)))
            (values (* sign value) end))))))

;;; ------------------------------------------------------------------------
;;; The standard macro characters (CLHS 2.4)

;;; `(` — a list, which may be dotted. A `.` on its own, followed by a
;;; delimiter, is the consing dot; any other `.` begins a token.
(defun %read-list (stream char)
  (%read-list-items stream nil))

(defun %read-list-items (stream items)
  (let ((char (%skip-whitespace stream)))
    (cond ((null char) (error 'end-of-file "end of file inside a list"))
          ((eq char #\)) (reverse items))
          ((and (eq char #\.) (%dot-alone-p stream))
           (if (null items) (error "a dot with nothing before it"))
           (%revappend items (%read-dotted-tail stream)))
          (t (let ((found (%read-maybe-nothing stream char)))
               (%read-list-items stream (if found (cons (car found) items) items)))))))

(defun %dot-alone-p (stream)
  (let ((next (peek-char nil stream nil nil)))
    (or (null next) (%terminatorp next))))

;;; After a consing dot: exactly one object, then the closing parenthesis,
;;; with only comments between.
(defun %read-dotted-tail (stream)
  (let ((tail (%read-required stream)))
    (%expect-close stream)
    tail))

(defun %expect-close (stream)
  (let ((char (%skip-whitespace stream)))
    (cond ((null char) (error 'end-of-file "end of file inside a list"))
          ((eq char #\)) nil)
          ((%read-maybe-nothing stream char) (error "more than one object after a dot"))
          (t (%expect-close stream)))))

(defun %revappend (items tail)
  (if (null items) tail (%revappend (cdr items) (cons (car items) tail))))

;;; `(read-delimited-list char stream)` — objects up to CHAR, as a list. What
;;; a program's own bracket-like reader macro wants.
(defun read-delimited-list (char &optional stream recursive-p)
  (%read-delimited (%input-stream stream) char nil))

(defun %read-delimited (stream close items)
  (let ((char (%skip-whitespace stream)))
    (cond ((null char) (error 'end-of-file "end of file inside a delimited list"))
          ((eq char close) (reverse items))
          (t (let ((found (%read-maybe-nothing stream char)))
               (%read-delimited stream close (if found (cons (car found) items) items)))))))

(defun %read-close (stream char)
  (error "a close parenthesis with no open one"))

;;; `'x` — `(quote x)`.
(defun %read-quote (stream char)
  (list 'quote (%read-required stream)))

;;; `;` to the end of the line, which reads as nothing.
(defun %read-comment (stream char)
  (do ((next (%next-char stream) (%next-char stream)))
      ((or (null next) (eq next #\Newline)) (values))))

;;; `"..."` — a string, in which `\` escapes the next character and nothing
;;; else is special. It is a literal, so it is made constant: immutable (#26)
;;; and packed (#44), as the Rust reader makes it.
;;; TABLE says whether the readtable has a syntax table, asked once per
;;; string: without one the escape is `\`, with one whichever characters are
;;; single escapes now.
(defun %read-string (stream close)
  (let ((table (%readtable-syntax *readtable*))
        (chars nil)
        (count 0))
    (do ((char (%next-char stream) (%next-char stream)))
        ((eq char close) (%constant-string (%chars-string chars count)))
      (if (null char) (error 'end-of-file "end of file inside a string"))
      (if (if table (eq (%char-syntax char) :single-escape) (eq char #\\))
          (setq char (read-char stream)))
      (setq chars (cons char chars)
            count (+ count 1)))))

;;; Backquote. While a template is being read, `%backquote-depth` counts how
;;; many backquotes enclose it, so that a comma outside every one is an
;;; error. A comma reads as a marker list, `(marker form)`; when the template
;;; is complete, `%quasiquote` turns it into the code that builds it.
;;;
;;; The markers are uninterned symbols, so no input can name them: a marker
;;; a program could type would let `(a (marker b))` expand as if a comma had
;;; been written. The Rust reader gets the same effect from names with a
;;; space in them, which its tokens cannot contain; this reader has `|...|`,
;;; which can.
(defvar %backquote-depth 0)
(defvar %unquote-marker (make-symbol "UNQUOTE"))
(defvar %splice-marker (make-symbol "SPLICE"))

(defun %read-backquote (stream char)
  (let ((%backquote-depth (+ %backquote-depth 1)))
    (%quasiquote (%read-required stream))))

(defun %read-comma (stream char)
  (if (eq %backquote-depth 0) (error "a comma outside a backquote"))
  (let ((splicing (eq (peek-char nil stream nil nil) #\@)))
    (if splicing (read-char stream))
    (list (if splicing %splice-marker %unquote-marker) (%read-required stream))))

;;; Exactly `reader.rs::quasiquote`, rule for rule:
;;;
;;;   `atom          =>  (quote atom)
;;;   `,x            =>  x
;;;   `(a . rest)    =>  (cons `a `rest)
;;;   `(,@x . rest)  =>  (append x `rest)
(defun %quasiquote (form)
  (cond ((not (consp form)) (list 'quote form))
        ((eq (car form) %unquote-marker) (cadr form))
        (t (let ((rest (%quasiquote (cdr form)))
                 (head (car form)))
             (if (and (consp head) (eq (car head) %splice-marker))
                 (list 'append (cadr head) rest)
                 (list 'cons (%quasiquote head) rest))))))

;;; `#` — read an optional decimal argument, then the sub-character, and call
;;; the function the dispatch table has for it with both.
(defun %read-dispatch (stream char)
  (%read-dispatch-argument stream char nil))

(defun %read-dispatch-argument (stream char argument)
  (let* ((sub-char (read-char stream))
         (weight (digit-char-p sub-char)))
    (if weight
        (%read-dispatch-argument stream char (+ (* (if argument argument 0) 10) weight))
        (let ((function (gethash (char-upcase sub-char) (%dispatch-table char *readtable*))))
          (cond ((and *read-suppress* (not (member sub-char '(#\( #\| #\+ #\- #\\))))
                 ;; Suppressed, every other `#x` reads as nothing but the
                 ;; object after it (`#*101`, `#C(1 2)`, `#P"..."`), except
                 ;; `#1#`, which has none (CLHS 2.4.8).
                 (if (not (eql sub-char #\#)) (%read-required stream))
                 nil)
                (function (funcall function stream sub-char argument))
                (t (error 'reader-error "no dispatch function for that character after #" sub-char)))))))

;;; `#\x` — a character. `#\a` is one character; `#\Space` is a name, which
;;; is what more letters after the first mean, looked up by `name-char`.
(defun %read-sharp-backslash (stream sub-char argument)
  (let ((first (read-char stream)))
    (if (and (alphanumericp first)
             (let ((next (peek-char nil stream nil nil)))
               (and next (not (%terminatorp next)))))
        (let ((name (%read-name stream (list (char-upcase first)) 1)))
          (cond (*read-suppress* nil)
                ((name-char name))
                (t (error "no character by that name" name))))
        first)))

;;; `#+feature form` reads FORM if the feature expression is true, and as
;;; nothing otherwise; `#-` the other way round (CLHS 2.4.8.17). A feature
;;; is read as a keyword, and combined with `and`, `or` and `not`. A form
;;; skipped is still read, with `*read-suppress*`, so that its end is found
;;; whatever it contains.
(defun %read-sharp-feature (stream sub-char argument)
  (let ((test (let ((*package* (find-package "KEYWORD"))) (%read-required stream))))
    (if (and (not *read-suppress*)
             (eq (if (%feature-true-p test) t nil) (eql sub-char #\+)))
        (%read-required stream)
        (progn (let ((*read-suppress* t)) (%read-required stream))
               (values)))))

(defun %feature-true-p (test)
  (cond ((symbolp test) (member test *features*))
        ((and (consp test) (eq (car test) :and)) (every #'%feature-true-p (cdr test)))
        ((and (consp test) (eq (car test) :or)) (some #'%feature-true-p (cdr test)))
        ((and (consp test) (eq (car test) :not)) (not (%feature-true-p (cadr test))))
        (t (error "not a feature expression" test))))

;;; `#.form` — FORM evaluated as the reader reads it (CLHS 2.4.8.6), unless
;;; `*read-eval*` forbids it, which is how a program reads untrusted text.
(defun %read-sharp-dot (stream sub-char argument)
  (let ((form (%read-required stream)))
    (cond (*read-suppress* nil)
          (*read-eval* (eval form))
          (t (error "#. is not allowed while *read-eval* is false")))))

(defun %read-name (stream chars count)
  (let ((char (read-char stream nil nil)))
    (cond ((null char) (%chars-string chars count))
          ((%terminatorp char)
           (unread-char char stream)
           (%chars-string chars count))
          (t (%read-name stream (cons (char-upcase char) chars) (+ count 1))))))


;;; `#'f` — `(function f)`.
(defun %read-sharp-quote (stream sub-char argument)
  (list 'function (%read-required stream)))

;;; `#x1F`, `#b101`, `#o17`, `#36rZ` — a rational in a radix (CLHS 2.4.8.7
;;; to 2.4.8.10), here an integer, of any size. Added with bignums (#16):
;;; the printer has written `#x` prefixes since step G, and now they read
;;; back.
(defun %read-sharp-radix (stream sub-char argument)
  (let ((radix (cond ((char-equal sub-char #\X) 16)
                     ((char-equal sub-char #\B) 2)
                     ((char-equal sub-char #\O) 8)
                     ((and argument (<= 2 argument 36)) argument)
                     (t (error "#R wants a radix from 2 to 36" argument))))
        (chars nil))
    (do ((next (peek-char nil stream nil nil) (peek-char nil stream nil nil)))
        ((or (null next) (%terminatorp next)))
      (push (read-char stream) chars))
    (%integer-in-radix (coerce (reverse chars) 'string) radix)))

(defun %integer-in-radix (text radix)
  (let* ((negative (and (> (length text) 0) (eq (char text 0) #\-)))
         (start (if (and (> (length text) 0) (member (char text 0) '(#\- #\+))) 1 0))
         (value 0))
    (if (>= start (length text)) (error "no digits after the radix" text))
    (do ((i start (+ i 1))) ((>= i (length text)))
      (let ((weight (digit-char-p (char text i) radix)))
        (if (null weight) (error "not a digit in this radix" text radix))
        (setq value (+ (* value radix) weight))))
    (if negative (- value) value)))

;;; `#(...)` — a vector of the elements.
(defun %read-sharp-paren (stream sub-char argument)
  (apply #'vector (%read-list-items stream nil)))

;;; `#*1011` — a bit vector (#67). With a length, `#5*101` is five bits, the
;;; last digit repeated to fill them (CLHS 2.4.8.4).
(defun %read-sharp-star (stream sub-char argument)
  (let ((bits nil))
    (do ((char (peek-char nil stream nil nil) (peek-char nil stream nil nil)))
        ((not (and char (member char '(#\0 #\1)))))
      (push (if (eql (read-char stream) #\1) 1 0) bits))
    (setq bits (nreverse bits))
    (let ((next (peek-char nil stream nil nil)))
      (if (and next (not (%terminatorp next)))
          (error "a bit vector holds only 0 and 1")))
    (cond ((null argument) (coerce bits 'bit-vector))
          ((> (length bits) argument) (error "more bits than the length given"))
          ((and (null bits) (> argument 0)) (error "a length but no bits to fill it with"))
          (t (let ((vector (make-array argument :element-type 'bit
                                                :initial-element (if bits (car (last bits)) 0))))
               (dotimes (i (length bits) vector)
                 (setf (aref vector i) (nth i bits))))))))

;;; `#2A((1 2) (3 4))` — an array of that rank, its elements nested as deep
;;; as it has dimensions; each dimension is the length at that depth.
(defun %read-sharp-a (stream sub-char argument)
  (let ((contents (%read-required stream)))
    (if (null argument) (error "#A wants a rank: #2A"))
    (let ((dimensions nil) (level contents))
      (dotimes (i argument)
        (push (length level) dimensions)
        (setq level (if (> (length level) 0) (elt level 0) nil)))
      (make-array (nreverse dimensions) :initial-contents contents))))

;;; `#S(name slot value ...)` — a structure, made by calling its standard
;;; constructor, `make-NAME`, with each slot name as a keyword (CLHS
;;; 2.4.8.13). Quokka's structures are vectors (tier 0), and print as one,
;;; so the printer never writes `#S`; reading it is what a program written
;;; for another Lisp needs.
(defun %read-sharp-s (stream sub-char argument)
  (let ((form (%read-required stream)))
    (cond (*read-suppress* nil)
          ((not (and (consp form) (symbolp (car form))))
           (error 'reader-error "#S wants (name slot value ...)" form))
          (t (apply (%symbol-concat 'make- (car form))
                    (do ((rest (cdr form) (cddr rest))
                         (args nil (list* (cadr rest) (intern (string (car rest)) "KEYWORD") args)))
                        ((null rest) (nreverse args))))))))

;;; `#:name` — a symbol in no package, new every time it is read.
(defun %read-sharp-colon (stream sub-char argument)
  (multiple-value-bind (text escaped colons) (%start-token stream (read-char stream))
    (if colons (error "a package marker in an uninterned symbol") (make-symbol text))))

;;; `#| ... |#` — a comment, which nests, and reads as nothing.
(defun %read-block-comment (stream sub-char argument)
  (%skip-block-comment stream 1)
  (values))

(defun %skip-block-comment (stream depth)
  (if (> depth 0)
      (let ((char (read-char stream)))
        (cond ((and (eq char #\|) (eq (peek-char nil stream nil nil) #\#))
               (read-char stream)
               (%skip-block-comment stream (- depth 1)))
              ((and (eq char #\#) (eq (peek-char nil stream nil nil) #\|))
               (read-char stream)
               (%skip-block-comment stream (+ depth 1)))
              (t (%skip-block-comment stream depth))))))

;;; The standard readtable, and `*readtable*` as a copy of it.
(set-macro-character #\( #'%read-list nil %standard-readtable)
(set-macro-character #\) #'%read-close nil %standard-readtable)
(set-macro-character #\' #'%read-quote nil %standard-readtable)
(set-macro-character #\; #'%read-comment nil %standard-readtable)
(set-macro-character #\" #'%read-string nil %standard-readtable)
(set-macro-character #\` #'%read-backquote nil %standard-readtable)
(set-macro-character #\, #'%read-comma nil %standard-readtable)
(make-dispatch-macro-character #\# t %standard-readtable)
(set-dispatch-macro-character #\# #\\ #'%read-sharp-backslash %standard-readtable)
(set-dispatch-macro-character #\# #\' #'%read-sharp-quote %standard-readtable)
(set-dispatch-macro-character #\# #\( #'%read-sharp-paren %standard-readtable)
(set-dispatch-macro-character #\# #\* #'%read-sharp-star %standard-readtable)
(set-dispatch-macro-character #\# #\A #'%read-sharp-a %standard-readtable)
(set-dispatch-macro-character #\# #\: #'%read-sharp-colon %standard-readtable)
(set-dispatch-macro-character #\# #\S #'%read-sharp-s %standard-readtable)
(set-dispatch-macro-character #\# #\| #'%read-block-comment %standard-readtable)
(set-dispatch-macro-character #\# #\x #'%read-sharp-radix %standard-readtable)
(set-dispatch-macro-character #\# #\b #'%read-sharp-radix %standard-readtable)
(set-dispatch-macro-character #\# #\o #'%read-sharp-radix %standard-readtable)
(set-dispatch-macro-character #\# #\r #'%read-sharp-radix %standard-readtable)
(set-dispatch-macro-character #\# #\+ #'%read-sharp-feature %standard-readtable)
(set-dispatch-macro-character #\# #\- #'%read-sharp-feature %standard-readtable)
(set-dispatch-macro-character #\# #\. #'%read-sharp-dot %standard-readtable)

(defvar *readtable* (copy-readtable %standard-readtable))
