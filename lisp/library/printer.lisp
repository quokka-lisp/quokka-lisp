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

(defstruct %string-output-stream chars count fresh closed)
(defstruct %console-stream fresh)

;;; `close` of a string stream (`gray-streams.lisp`, which is loaded before
;;; these structures exist and so cannot `setf` their slots): it is closed
;;; from now on, as in SBCL. The console is never closed.
(defun %close-structure-stream (stream)
  (cond ((%string-input-stream-p stream) (setf (%string-input-stream-closed stream) t))
        ((%string-output-stream-p stream) (setf (%string-output-stream-closed stream) t))))

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

;;; The other standard streams (#71). CLHS has a stream for the terminal,
;;; one each for errors, tracing and the debugger, and one for questions to
;;; the user, which a program may bind to send output elsewhere. Tier 1 has
;;; one place to write to, so each is the console stream; `*terminal-io*`,
;;; `*debug-io*` and `*query-io*` are two-way streams in CLHS, and reading
;;; from them reads the console (`%input-stream`, #131): they are a console
;;; stream of their own, `%console-io`, so that `*standard-output*` is not an
;;; input stream, as in SBCL.
(defvar %console-io (make-%console-stream :fresh t))
(defvar *terminal-io* %console-io)
(defvar *error-output* *standard-output*)
(defvar *trace-output* *standard-output*)
(defvar *debug-io* %console-io)
(defvar *query-io* %console-io)

;;; An output stream designator: nil means standard output, and t the
;;; terminal, `*terminal-io*`.
(defun %output-stream (designator)
  (cond ((null designator) *standard-output*)
        ((eq designator t) *terminal-io*)
        ((or (%string-output-stream-p designator) (%console-stream-p designator)) designator)
        ;; A Gray stream (`gray-streams.lisp`, #136): the functions below
        ;; send it to the generic functions after their own two kinds.
        ((%gray-stream-p designator) designator)
        (t (error 'type-error :datum designator :expected-type 'stream))))

;;; `input-stream-p` and `output-stream-p` are generic functions in
;;; `gray-streams.lisp`, as in SBCL.

;;; Every write comes down to this: characters START to END of STRING.
(defun %stream-write (stream string start end)
  (if (< start end)
      (cond ((%console-stream-p stream)
             (%write-console (if (and (eq start 0) (eq end (length string))
                                      (simple-string-p string))
                                 string
                                 (%substring string start end)))
             (setf (%console-stream-fresh stream) (eq (aref string (- end 1)) #\Newline)))
            ((%string-output-stream-p stream)
             (setf (%string-output-stream-chars stream)
                   (%push-chars string start end (%string-output-stream-chars stream)))
             (setf (%string-output-stream-count stream)
                   (+ (%string-output-stream-count stream) (- end start)))
             (setf (%string-output-stream-fresh stream) (eq (aref string (- end 1)) #\Newline)))
            (t (stream-write-string stream string start end))))
  string)

(defun %push-chars (string start end chars)
  (if (< start end)
      (%push-chars string (+ start 1) end (cons (aref string start) chars))
      chars))

(defun %fresh-p (stream)
  (cond ((%console-stream-p stream) (%console-stream-fresh stream))
        ((%string-output-stream-p stream) (%string-output-stream-fresh stream))
        (t (stream-start-line-p stream))))

(defun write-string (string &optional stream &key (start 0) end)
  (%stream-write (%output-stream stream) string start (if end end (length string)))
  string)

(defun write-line (string &optional stream &key (start 0) end)
  (let ((stream (%output-stream stream)))
    (%stream-write stream string start (if end end (length string)))
    (terpri stream)
    string))

;;; One character. Into a string stream it is one cons, directly: by way
;;; of a one-character string and `%stream-write` it was 310 steps a
;;; character, and `format` writes its literal text a character at a time
;;; (#112).
(defun write-char (char &optional stream)
  (let ((stream (%output-stream stream)))
    (cond ((%string-output-stream-p stream)
           (setf (%string-output-stream-chars stream)
                 (cons char (%string-output-stream-chars stream)))
           (setf (%string-output-stream-count stream)
                 (+ (%string-output-stream-count stream) 1))
           (setf (%string-output-stream-fresh stream) (eq char #\Newline)))
          ((%console-stream-p stream)
           (%stream-write stream (%make-string 1 char) 0 1))
          (t (stream-write-char stream char))))
  char)

(defun terpri (&optional stream)
  (let ((stream (%output-stream stream)))
    (if (%structure-stream-p stream)
        (write-char #\Newline stream)
        (stream-terpri stream)))
  nil)

;;; A newline unless the stream is already at the start of a line, and
;;; whether one was written.
(defun fresh-line (&optional stream)
  (let ((stream (%output-stream stream)))
    (cond ((%structure-stream-p stream)
           (if (%fresh-p stream) nil (progn (terpri stream) t)))
          (t (stream-fresh-line stream)))))

;;; Nothing the library's own streams write is buffered — a console write is
;;; sent at once — so there is nothing to finish, force or clear; a Gray
;;; stream is asked.
(defun finish-output (&optional stream)
  (let ((stream (%output-stream stream)))
    (if (%structure-stream-p stream) nil (stream-finish-output stream))))
(defun force-output (&optional stream)
  (let ((stream (%output-stream stream)))
    (if (%structure-stream-p stream) nil (stream-force-output stream))))
(defun clear-output (&optional stream)
  (let ((stream (%output-stream stream)))
    (if (%structure-stream-p stream) nil (stream-clear-output stream))))

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

;;; While the printer runs a `print-object` method, the depth the object was
;;; printed at; nil otherwise. A `prin1` or `format` inside the method then
;;; continues the printout it is part of, one level counted from there, as
;;; SBCL's does (#124): `(#<BOX (1 #)>)` under `*print-level*` 2.
(defvar %print-nesting nil)

;;; True while `*print-circle*`'s first pass runs a method to see what it
;;; would print: everything printed is then noted, not written.
(defvar %noting nil)

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
    (%print-to-string object)))

;;; OBJECT as text, under the printer variables as they stand. What
;;; `write-to-string` comes to once its keywords are bound, and what
;;; `prin1-to-string` and `princ-to-string` call directly: each of those
;;; differs from the current settings in one or two variables, and going
;;; through `write-to-string` to say so parsed fifteen keywords and made
;;; fifteen dynamic bindings on every call (#112).
(defun %print-to-string (object)
  (let ((out (make-string-output-stream)))
    (if *print-readably*
        ;; CLHS 22.1.3: printing readably implies escapes, arrays and
        ;; gensym prefixes, and nothing cut short by length or level.
        (let ((*print-escape* t) (*print-array* t) (*print-gensym* t)
              (*print-length* nil) (*print-level* nil) (*print-lines* nil))
          (%print-top object out))
        (%print-top object out))
    (get-output-stream-string out)))

;;; `princ` is `write` with neither escapes nor readability (CLHS 22.4).
(defun prin1-to-string (object)
  (let ((*print-escape* t))
    (%print-to-string object)))

(defun princ-to-string (object)
  (let ((*print-escape* nil) (*print-readably* nil))
    (%print-to-string object)))

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

;;; `(print-unreadable-object (object stream &key type identity) body...)`
;;; (#68): `#<`, the object's type if TYPE, what BODY writes, and `>`. The
;;; spacing is SBCL's, measured: the type is always followed by a space,
;;; body or not (`#<POINT >`), and the identity is preceded by one when
;;; there is a body or no type. Quokka's objects move and have no address,
;;; so the identity's own text is empty: only its space is written (#124).
;;;
;;; It is no level of nesting: what BODY prints is printed at the depth the
;;; object was, as in SBCL when it is not pretty-printing (measured; its
;;; pretty printer does count one, and Quokka has none yet).
(defmacro print-unreadable-object (spec &body body)
  (destructuring-bind (object stream &key type identity) spec
    `(%print-unreadable-object ,object ,stream ,type ,identity
                               ,(if body `(lambda () ,@body) nil))))

(defun %print-unreadable-object (object stream type identity body)
  (let ((out (%output-stream stream)))
    (if *print-readably*
        (error 'print-not-readable "this object cannot be printed readably" object))
    (write-string "#<" out)
    (when type
      ;; The whole type, however deep the object itself is, and with a
      ;; package prefix only when escaping, as `prin1` and not `princ`
      ;; writes it: SBCL's `(write (type-of object) :circle nil :level nil
      ;; :length nil)`.
      (let ((*print-level* nil) (*print-length* nil) (*print-circle* nil))
        (%print-top (type-of object) out))
      (write-char #\Space out))
    (if body (funcall body))
    (if (and identity (or body (not type))) (write-char #\Space out))
    (write-char #\> out))
  nil)

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

;;; The top of printing one object. Every printout binds its own table,
;;; so a `prin1-to-string` called while printing starts afresh — unless it
;;; is called by a `print-object` method the printer is running: then it
;;; continues that printout, at that depth and with that table, or, in the
;;; first pass, only notes what it would print. A method that prints with
;;; `*print-circle*` false (`(write x :circle nil)`) prints with no labels
;;; and notes nothing, and one that turns it on in a printout without it
;;; starts a table of its own, as in SBCL.
(defun %print-top (object out)
  (cond ((and %noting *print-circle*) (%note-shared object %circle-table))
        (%print-nesting
         (let ((level %print-nesting) (%print-nesting nil))
           (cond ((not *print-circle*)
                  (let ((%circle-table nil) (%noting nil))
                    (%print-object object out level)))
                 (%circle-table (%print-object object out level))
                 (t (let ((%circle-table (%find-shared object)) (%circle-label 0))
                      (%print-object object out level))))))
        (t (let ((%circle-table (if *print-circle* (%find-shared object) nil))
                 (%circle-label 0))
             (%print-object object out 0)))))

(defun %shareable-p (object)
  (not (or (numberp object)
           (characterp object)
           (and (symbolp object) (symbol-package object)))))

;;; Does the printer write OBJECT's elements as an array's? Vectors, and
;;; arrays of several dimensions (#67), whose elements are walked in
;;; row-major order; a vector's are its active ones, up to its fill pointer.
(defun %printed-as-vector-p (object)
  (and (arrayp object)
       (not (stringp object))
       (not (bit-vector-p object))
       *print-array*))

;;; The library's own structures print as `#<...>`: nothing reads them back.

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
            (cond ((%plain-structure-p object)
                   (dotimes (i (%layout-length (%structure-layout object)))
                     (%note-shared (%structure-ref object i) table)))
                  ((%printed-by-method-p object) (%note-through-method object table))
                  ((consp object)
                   (%note-shared (car object) table)
                   (%note-shared (cdr object) table))
                  ((and (%printed-as-vector-p object) (vectorp object))
                   (dotimes (i (length object))
                     (%note-shared (aref object i) table)))
                  ((%printed-as-vector-p object)
                   (dotimes (i (array-total-size object))
                     (%note-shared (row-major-aref object i) table))))))))

;;; A structure that prints as `#S(...)` because no method of a program's
;;; applies to it: its slots are walked directly, as before `print-object`,
;;; which is cheaper than running the default method (#124).
(defun %plain-structure-p (object)
  (and (%structure-layout object)
       (not (%unreadable-structure-p object))
       (%printed-plainly-p object)))

;;; What a `print-object` method prints is up to the method, so the first
;;; pass runs it, into a stream nobody reads, and notes whatever it prints
;;; (SBCL's way too): `#1=#<BOX #1#>`.
(defun %note-through-method (object table)
  (let ((%circle-table table) (%noting t) (%print-nesting 0))
    (print-object object (make-string-output-stream))))

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
;;; In the first pass of `*print-circle*` (`%noting`), noted instead.
(defun %print-object (object out level)
  (cond ((null %circle-table) (%print-plain object out level))
        (%noting (%note-shared object %circle-table))
        (t (let ((mark (%circle-mark object)))
             (cond ((null mark) (%print-plain object out level))
                   ((integerp mark) (%print-label mark #\# out))
                   (t (setq %circle-label (+ %circle-label 1))
                      (setf (gethash object %circle-table) %circle-label)
                      (%print-label %circle-label #\= out)
                      (%print-plain object out level)))))))

;;; The text for an object nothing can read back: `#<...>`, or, when printing
;;; readably, the error CLHS asks for instead.
(defun %print-unreadable (object text out)
  (if *print-readably*
      (error 'print-not-readable "this object cannot be printed readably" object)
      (write-string text out)))

;;; Printing one object that is not shared: by its `print-object` method
;;; when it may have one of a program's (#124), by the printer's own code
;;; otherwise.
(defun %print-plain (object out level)
  (if %print-object-on-built-in
      (%print-with-method object out level)
      (%print-built-in object out level t)))

;;; Does the printer send OBJECT to `print-object`? Instances, structures and
;;; conditions always; anything else once a program has a method for a
;;; built-in class (`%print-object-on-built-in`).
(defun %printed-by-method-p (object)
  (if %print-object-on-built-in
      t
      (and (not (consp object))
           (or (%instance-wrapper object)
               (and (%structure-layout object) (not (%unreadable-structure-p object)))
               (%conditionp object)))))

(defun %print-with-method (object out level)
  (let ((%print-nesting level))
    (print-object object out)))

;;; The printer's own code: what the method on `t` runs. METHODS true sends
;;; instances, structures and conditions to their `print-object` methods,
;;; and is false only in the method on `t` itself, which must not send
;;; them back. The common kinds of object are tested first, and none of
;;; them pays for `print-object`.
(defun %print-built-in (object out level methods)
  (cond ((integerp object) (%print-integer object out))
        ((%ratiop object) (%print-ratio object out))
        ((floatp object) (%print-float object out))
        ((characterp object) (%print-character object out))
        ((symbolp object) (%print-symbol object out))
        ((stringp object) (%print-string object out))
        ((consp object) (%print-list object out level))
        ;; The library's own structures are not data anyone reads back, and
        ;; SBCL writes them as `#<...>`.
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
        ((%instance-wrapper object)
         (if methods
             (%print-with-method object out level)
             (%print-unreadable object (%printed object) out)))
        ((%structure-layout object)
         (if (and methods (not (%printed-plainly-p object)))
             (%print-with-method object out level)
             (%print-structure-slots object (%structure-layout object) out level)))
        ;; Not an array — a function, a condition, a hash table: nothing
        ;; reads them back, so CLHS lets each implementation print them its
        ;; own way, and this is the Rust printer's, so both agree. Or
        ;; `*print-array*` false: an array by its dimensions alone.
        ;; An array of element type nil has no elements to print (#77), so
        ;; it is printed as SBCL prints one, by its type.
        ((not (and (arrayp object) *print-array* (array-element-type object)))
         (cond ((arrayp object)
                (%print-unreadable object
                                   (%string-append (%string-append (if (array-element-type object)
                                                                       "#<(ARRAY "
                                                                       "#<(ARRAY NIL ")
                                                                   (princ-to-string (array-dimensions object)))
                                                   ")>")
                                   out))
               ((and methods (%conditionp object)) (%print-with-method object out level))
               (t (%print-unreadable object (%printed object) out))))
        ;; `#*1011`: every bit, `*print-length*` or not, as SBCL prints one.
        ((bit-vector-p object)
         (write-string "#*" out)
         (dotimes (i (length object)) (write-char (if (eql (aref object i) 1) #\1 #\0) out)))
        ((vectorp object) (%print-vector object out level))
        (t (%print-array object out level))))

;;; Would the generic function print OBJECT, a structure, exactly as
;;; `#S(...)`, because no method applies to it but the library's own on
;;; `structure-object` and `t`? A generic function call costs a class lookup
;;; and a search of its cache, which made printing a list of structures a
;;; third slower; so the answer is kept for each structure type, with the
;;; dispatch epoch it was found in, and found again when a class or a
;;; method has changed since (#124).
(defvar %plain-structures (make-hash-table :test 'eq))
(defvar %default-print-methods nil)

(defun %printed-plainly-p (object)
  (let* ((layout (%structure-layout object))
         (entry (gethash layout %plain-structures)))
    (if (and entry (eq (car entry) %dispatch-epoch))
        (cdr entry)
        (let ((plain (%only-default-print-methods-p object)))
          (setf (gethash layout %plain-structures) (cons %dispatch-epoch plain))
          plain))))

;;; No `eql` method and no method on a particular kind of stream, which a
;;; layout cannot answer for, and every method that applies to OBJECT's
;;; class is one of the two defaults.
(defun %only-default-print-methods-p (object)
  (let ((gf (%defined-function 'print-object)))
    (and %default-print-methods
         (null (%gf-eql-positions gf))
         (let ((cpl (%class-precedence-list (class-of object))))
           (dolist (method (%gf-methods gf) t)
             (let ((specializers (%method-specializers method)))
               (unless (eq (second specializers) (find-class t)) (return nil))
               (when (and (member (first specializers) cpl)
                          (not (member method %default-print-methods)))
                 (return nil))))))))

;;; A structure as `#S(POINT :X 1 :Y 2)`, which `#S` reads back: what the
;;; method on `structure-object` prints. `#S` counts as a level of nesting
;;; and each slot as one element for `*print-length*`, as in SBCL. A
;;; `defstruct`'s `:print-function` or `:print-object` is a method of its
;;; own (`%note-structure-defined`, below).
(defun %print-structure-slots (object layout out level)
  (if (%too-deep-p level)
      (write-char #\# out)
      (let ((count 0))
        (write-string "#S(" out)
        (%print-object (%layout-name layout) out (+ level 1))
        (dolist (slot (%layout-slots layout))
          (write-char #\Space out)
          (if (%too-long-p count)
              (progn (write-string "..." out) (return)))
          ;; A keyword, with its colon even under `princ`, as SBCL writes it.
          (if (not *print-escape*) (write-char #\: out))
          (%print-object (intern (symbol-name (first slot)) "KEYWORD") out (+ level 1))
          (write-char #\Space out)
          (%print-object (%structure-ref object (second slot)) out (+ level 1))
          (setq count (+ count 1)))
        (write-char #\) out))))

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
                ;; In the first pass of `*print-circle*`, through a method
                ;; (#124): each tail is noted as it is walked into, as
                ;; `%note-shared` notes one, so that a shared tail is found
                ;; and a circular list ends.
                ((and %noting (consp rest))
                 (if (gethash rest %circle-table)
                     (setf (gethash rest %circle-table) :many)
                     (progn (setf (gethash rest %circle-table) :once)
                            (%print-list-items rest out level (+ count 1)))))
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

;;; Each run of characters up to the next one that wants a backslash goes
;;; to the stream in one write. One write per character was most of what
;;; printing a long string cost, and a structure's accessors made each
;;; write dearer (#73).
(defun %print-escaped (string out at end quote)
  (let ((stop (do ((i at (+ i 1)))
                  ((or (>= i end) (eq (aref string i) quote) (eq (aref string i) #\\)) i))))
    (%stream-write out string at stop)
    (if (< stop end)
        (progn (write-char #\\ out)
               (write-char (aref string stop) out)
               (%print-escaped string out (+ stop 1) end quote)))))

;;; `#\a`, `#\Tab`, and `#\ ` for a space, as SBCL writes it.
(defun %print-character (char out)
  (if *print-escape*
      (progn (write-string "#\\" out)
             ;; As SBCL: an ASCII graphic character as itself, and any other
             ;; by its name if it has one (#70). Only Latin-1 characters have
             ;; names in Quokka, so above 255 the character itself is
             ;; written, where SBCL writes a Unicode name; both read back.
             (if (and (graphic-char-p char) (< (char-code char) 128))
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

;;; `1/3`, in `*print-base*`. Under `*print-radix*` a ratio has its radix
;;; prefix in every base, decimal included: `#10r1/3` (CLHS 22.1.3.2), since
;;; a trailing point cannot mark a ratio as it marks a decimal integer.
(defun %print-ratio (ratio out)
  (if *print-radix* (write-string (%radix-prefix *print-base*) out))
  (if (minusp ratio) (write-char #\- out))
  (%print-digits (%integer-digits (%ratio-numerator ratio) *print-base* nil) out)
  (write-char #\/ out)
  (%print-digits (%integer-digits (%ratio-denominator ratio) *print-base* nil) out))

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
;;; is written in `*print-case*`, as the readtable's case allows.
(defun %print-name (name out)
  (if (and *print-escape* (%needs-bars-p name))
      (progn (write-char #\| out)
             (%print-escaped name out 0 (length name) #\|)
             (write-char #\| out))
      (%print-cased name out)))

(defun %current-case () (%readtable-case (%the-readtable *readtable*)))

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
;;; contains a letter the readtable's case would fold (a lower-case one in
;;; `:upcase`, an upper-case one in `:downcase`; none in `:preserve` and
;;; `:invert`, where what is printed reads back as it was), whitespace, a
;;; macro character, a colon or an escape. SBCL counts `#` anywhere, though the reader only
;;; dispatches on it at the start of a token; matching SBCL wins.
(defun %needs-bars-p (name)
  (let ((end (length name)))
    (or (eq end 0)
        (%all-dots-p name 0 end)
        (%token-integer name 10)
        (%ratio-or-float-p name)
        (and (not (eq *print-base* 10)) (%digits-in-base-p name *print-base*))
        (%potential-number-p name end *print-base*)
        (%special-char-in-p name 0 end (%current-case)))))

;;; Is NAME a *potential number* (CLHS 2.3.1.1)? Text with the shape of a
;;; number that is not one today: `+1A`, `1_`, `^2`, `1E5X`, `1/A`. The
;;; standard reserves such tokens — a later standard, or an implementation,
;;; may make them numbers — so a symbol named like one is written in bars,
;;; as SBCL writes it. In a base above ten the letters that are digits count
;;; as digits, which is what `A/B` in base 16 turns on (#114). The rules:
;;; only digits, signs, `/`, `.`, `^`, `_` and lone letters; at least one
;;; digit; a digit, sign, point or extension character first; no sign last.
(defun %potential-number-p (name end base)
  (and (> end 0)
       (let ((first (aref name 0)))
         (or (digit-char-p first base) (member first '(#\+ #\- #\. #\^ #\_))))
       (not (member (aref name (- end 1)) '(#\+ #\-)))
       (%potential-body-p name 0 end base nil)))

(defun %potential-body-p (name at end base digit-seen)
  (if (< at end)
      (let ((char (aref name at)))
        (cond ((digit-char-p char base) (%potential-body-p name (+ at 1) end base t))
              ((member char '(#\+ #\- #\/ #\. #\^ #\_))
               (%potential-body-p name (+ at 1) end base digit-seen))
              ((and (alpha-char-p char)
                    (not (%marker-at-p name (- at 1) end base))
                    (not (%marker-at-p name (+ at 1) end base)))
               (%potential-body-p name (+ at 1) end base digit-seen))
              (t nil)))
      digit-seen))

;;; Is the character at AT a letter that is not a digit in BASE?
(defun %marker-at-p (name at end base)
  (and (>= at 0) (< at end)
       (alpha-char-p (aref name at))
       (not (digit-char-p (aref name at) base))))

(defun %digits-in-base-p (name base)
  (let* ((end (length name))
         (start (%skip-sign name 0 end)))
    (and (< start end) (%all-digits-in-base-p name start end base))))

(defun %all-digits-in-base-p (name at end base)
  (if (< at end)
      (and (digit-char-p (aref name at) base) (%all-digits-in-base-p name (+ at 1) end base))
      t))

(defun %special-char-in-p (name at end case)
  (and (< at end)
       (let ((char (aref name at)))
         (or (and (eq case :upcase) (lower-case-p char))
             (and (eq case :downcase) (upper-case-p char))
             (eq char #\:)
             (not (eq (%char-syntax char) :constituent))
             (%special-char-in-p name (+ at 1) end case)))))

;;; How the letters of a name are written, from the readtable's case and
;;; `*print-case*` together (CLHS 22.1.3.3.2):
;;;
;;; - `:upcase`: `*print-case*` applies to the upper-case letters, which are
;;;   the ones the reader folds to; lower-case letters are written as they
;;;   are. `:downcase` lowers them all; `:capitalize` keeps the first letter
;;;   of each word — a run of letters and digits — and lowers the rest, so
;;;   `FOO-BAR` becomes `Foo-Bar` and `X2Y` `X2y`.
;;; - `:downcase`: the mirror image. `*print-case*` applies to the lower-case
;;;   letters: `:upcase` raises them, `:capitalize` raises the first of each
;;;   word.
;;; - `:preserve`: every letter as it is; `*print-case*` is ignored.
;;; - `:invert`: a name whose letters are all of one case is written in the
;;;   other, a mixed one as it is; `*print-case*` is ignored. What the reader
;;;   inverts, the printer inverts back.
(defun %print-cased (name out)
  (let ((case (%current-case))
        (end (length name)))
    (cond ((eq case :preserve) (write-string name out))
          ((eq case :invert) (%print-inverted name out))
          ((eq *print-case* :downcase) (%print-folded name out 0 end case #'char-downcase nil))
          ((eq *print-case* :upcase) (%print-folded name out 0 end case #'char-upcase nil))
          (t (%print-folded name out 0 end case nil t)))))

;;; Write NAME, passing each letter of the case CASE folds *to* through FOLD.
;;; FOLD nil is `:capitalize`: the first letter of a word upper case, the
;;; rest lower. Letters of the other case are written as they are.
(defun %print-folded (name out at end case fold word-start)
  (if (< at end)
      (let* ((char (aref name at))
             (mine (if (eq case :upcase) (upper-case-p char) (lower-case-p char))))
        (write-char (cond ((not mine) char)
                          (fold (funcall fold char))
                          (word-start (char-upcase char))
                          (t (char-downcase char)))
                    out)
        (%print-folded name out (+ at 1) end case fold (not (alphanumericp char))))))

(defun %print-inverted (name out)
  (let ((upper (some #'upper-case-p name))
        (lower (some #'lower-case-p name)))
    (cond ((and upper lower) (write-string name out))
          (upper (dotimes (i (length name)) (write-char (char-downcase (aref name i)) out)))
          (t (dotimes (i (length name)) (write-char (char-upcase (aref name i)) out))))))

;;; ------------------------------------------------------------------------
;;; `print-object` (CLOS sub-step 4, #124)
;;;
;;; The generic function a program specialises to say how its objects
;;; print (CLHS 22.1.3). The printer calls it for instances, structures and
;;; conditions (`%printed-by-method-p`), and the method on `t` is the
;;; printer's own code, so a method that calls `call-next-method` gets what
;;; the printer would have printed. Each method returns the object, as
;;; CLHS asks.

(defgeneric print-object (object stream))

(defmethod print-object ((object t) stream)
  (%print-built-in object stream (or %print-nesting 0) nil)
  object)

;;; `#<POINT>`. SBCL adds the address, which Quokka cannot show: objects
;;; move. So unlike `print-unreadable-object` with `:type t`, no space
;;; follows the name.
(defmethod print-object ((object standard-object) stream)
  (%print-unreadable-object object stream nil nil
                            (lambda () (prin1 (class-name (class-of object)) stream)))
  object)

(defmethod print-object ((object structure-object) stream)
  (%print-structure-slots object (%structure-layout object) stream (or %print-nesting 0))
  object)

;;; The two methods `%printed-plainly-p` looks for. Set again whenever this
;;; file loads, since a warm rebuild replaces both with new method objects.
(setq %default-print-methods
      (list (find-method #'print-object nil (list (find-class t) (find-class t)))
            (find-method #'print-object nil (list (find-class 'structure-object) (find-class t)))))

;;; The metaobjects print as SBCL's do, less the address:
;;; `#<STANDARD-CLASS POINT>`, `#<STANDARD-GENERIC-FUNCTION AREA (2)>` with
;;; its number of methods, `#<STANDARD-METHOD AREA :AROUND (CIRCLE)>`.
(defmethod print-object ((object class) stream)
  (print-unreadable-object (object stream :type t)
    (prin1 (class-name object) stream))
  object)

(defmethod print-object ((object generic-function) stream)
  (print-unreadable-object (object stream :type t)
    (prin1 (%gf-name object) stream)
    (write-string " (" stream)
    (prin1 (length (%gf-methods object)) stream)
    (write-string ")" stream))
  object)

(defmethod print-object ((object method) stream)
  (print-unreadable-object (object stream :type t)
    (let ((gf (%method-generic-function object)))
      (if gf (prin1 (%gf-name gf) stream) (write-string "(no generic function)" stream)))
    (dolist (qualifier (%method-qualifiers object))
      (write-char #\Space stream)
      (prin1 qualifier stream))
    (write-char #\Space stream)
    (prin1 (mapcar (lambda (specializer)
                     (if (%eql-specializer-p specializer)
                         (list 'eql (%eql-specializer-object specializer))
                         (class-name specializer)))
                   (%method-specializers object))
           stream))
  object)

(defmethod print-object ((object slot-definition) stream)
  (print-unreadable-object (object stream :type t)
    (prin1 (%slot-definition-name object) stream))
  object)

(defmethod print-object ((object eql-specializer) stream)
  (print-unreadable-object (object stream :type t)
    (prin1 (%eql-specializer-object object) stream))
  object)

;;; A `defstruct` with `:print-function` or `:print-object` defines a
;;; `print-object` method on its class, as SBCL's does, so that the most
;;; specific printer wins by the ordinary rules: a structure's own printer
;;; over a method on a structure it includes, and a method on a structure
;;; that includes it over both. The method holds the function itself, and a
;;; later `defstruct` of the name without the option leaves it in place,
;;; as SBCL's does. Redefined here from `structures.lisp`, which loads
;;; before generic functions exist; no structure defined before this file
;;; has a printer option.
(defun %note-structure-defined (name)
  (setf (get name '%structure-defined) t)
  (let ((layout (%find-layout name)))
    (if (and layout (null (%layout-type layout)) (%layout-printer layout))
        (%add-structure-printer layout))))

(defun %add-structure-printer (layout)
  (let* ((printer (%layout-printer layout))
         (function (cdr printer)))
    (add-method (%defined-function 'print-object)
                (%make-method (find-class 'standard-method) nil
                              (list (%structure-class layout) (find-class t))
                              '(object stream)
                              (if (eq (car printer) :object)
                                  (lambda (args next)
                                    next
                                    (funcall function (first args) (second args)))
                                  (lambda (args next)
                                    next
                                    (funcall function (first args) (second args)
                                             (or %print-nesting 0))))
                              nil))))
