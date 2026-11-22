;;;; The reader in Lisp (#45): read, read-from-string, string input streams
;;;; and readtables.
;;;;
;;;; Tier 1, so this runs compiled and against SBCL. Each test is read twice:
;;;; the file by whichever reader runs the test, and the string inside it by
;;;; the Lisp reader. Three things are left out on purpose. Backquote, which
;;;; SBCL reads into its own internal forms where Quokka must match the Rust
;;;; reader instead, and `differential.rs` checks that against the whole
;;;; system's source. Ratios and floats, which Quokka refuses until steps L and
;;;; M. And printing a symbol from another package, which needs the Lisp
;;;; printer (step G), so those are compared with `eq` and by name.

;; Atoms.
(read-from-string "foo")
(read-from-string "  42  ")
(read-from-string "-17")
(read-from-string "+5")
(read-from-string "12.")
(read-from-string "1+")
(read-from-string "-")
(read-from-string "\"a \\\"quoted\\\" string\"")
(read-from-string "#\\a")
(char-code (read-from-string "#\\Space"))
(char-code (read-from-string "#\\newline"))
(read-from-string "#\\(")
(read-from-string ":keyword")
(eq (read-from-string ":keyword") :keyword)

;; Where reading stops: one whitespace character after a top-level object
;; is consumed; a parenthesis ending a token is not.
(multiple-value-list (read-from-string "a b"))
(multiple-value-list (read-from-string "a)"))
(multiple-value-list (read-from-string "(a b . c) d"))
(multiple-value-list (read-from-string "x y z" t nil :start 2))
(read-from-string "" nil :empty)
(read-from-string "   ; only a comment" nil :empty)
(read-from-string "")

;; Lists, dotted lists, quote, function, vectors, comments.
(read-from-string "(a (b c) () d)")
(read-from-string "(a . b)")
(read-from-string "(a b . (c d))")
(read-from-string "(a ; a comment
  b)")
(read-from-string "(a #| a block #| nested |# |# b)")
(read-from-string "(a . b ; after the dot
  )")
(read-from-string "'x")
(read-from-string "#'car")
(read-from-string "#(1 two \"three\")")
(read-from-string "(a .b)")
(read-from-string "(a")
(read-from-string ")")
(read-from-string "(. a)")
(read-from-string "(a . b c)")

;; Case and escapes.
(symbol-name (read-from-string "FooBar"))
(symbol-name (read-from-string "|Mixed Case|"))
(symbol-name (read-from-string "a\\bc"))
(symbol-name (read-from-string "ab|cd|ef"))
(symbolp (read-from-string "\\1"))
(symbolp (read-from-string "|12|"))
(read-from-string "...")

;; Uninterned symbols.
(symbol-name (read-from-string "#:gen"))
(symbol-package (read-from-string "#:gen"))
(eq (read-from-string "#:gen") (read-from-string "#:gen"))

;; Package prefixes, and *package*. A symbol written in this file is read by
;; whichever reader runs the file, and until step D that is the Rust reader in
;; Quokka, which ignores *package*; so where it matters, both sides of a
;; comparison are read from strings.
(eq (read-from-string "cl:car") 'car)
(eq (read-from-string "common-lisp::car") 'car)
(eq (read-from-string "cl-user::a-name-of-our-own") (read-from-string "a-name-of-our-own"))
(eq (read-from-string "::foo") :foo)
(package-name (defpackage :reader-test (:use :cl) (:export :out)))
(package-name (symbol-package (read-from-string "reader-test::inside")))
(eq (read-from-string "reader-test:out") (read-from-string "reader-test::out"))
(let ((*package* (find-package :reader-test)))
  (package-name (symbol-package (read-from-string "fresh"))))
(let ((*package* (find-package :reader-test)))
  (eq (read-from-string "car") 'car))
(read-from-string "no-such-package:x")
(read-from-string "cl:no-such-external-symbol-here")
(read-from-string "a:b:c")

;; String input streams.
(with-input-from-string (s "1 2 3")
  (list (read s) (read s) (read s) (read s nil :eof)))
(with-input-from-string (s "abc" :start 1)
  (list (read-char s) (peek-char nil s) (read-char s) (read-char s nil :eof)))
(let ((s (make-string-input-stream "xy")))
  (read-char s)
  (unread-char #\x s)
  (list (read-char s) (read-char s)))
(peek-char t (make-string-input-stream "   z"))
(peek-char #\c (make-string-input-stream "abcd"))
(read-char (make-string-input-stream ""))
(streamp (make-string-input-stream ""))
(streamp "a string")

;; Characters.
(char-code #\A)
(code-char 97)
(char-upcase #\a)
(char-downcase #\A)
(char-upcase #\1)
(upper-case-p #\A)
(lower-case-p #\A)
(both-case-p #\-)
(alpha-char-p #\z)
(alpha-char-p #\5)
(digit-char-p #\7)
(digit-char-p #\a)
(digit-char-p #\a 16)
(digit-char-p #\z 36)
(alphanumericp #\_)

;; Readtables: a reader macro of our own, in a copy of the readtable.
(readtablep *readtable*)
(let ((*readtable* (copy-readtable)))
  (set-macro-character #\! (lambda (stream char) (list 'bang (read stream t nil t))))
  (read-from-string "(a !b c)"))
(read-from-string "!b")
(let ((*readtable* (copy-readtable)))
  (set-macro-character #\] (get-macro-character #\)))
  (set-macro-character #\[ (lambda (stream char) (cons 'bracket (read-delimited-list #\] stream t))))
  (read-from-string "[1 2 [3]]"))
(let ((*readtable* (copy-readtable)))
  (set-dispatch-macro-character #\# #\? (lambda (stream char arg) (list 'query arg)))
  (list (read-from-string "#?") (read-from-string "#7?")))
(let ((*readtable* (copy-readtable)))
  (set-macro-character #\% (lambda (stream char) (values)))
  (read-from-string "(a % b)"))
(eq (get-macro-character #\a) nil)
(null (get-dispatch-macro-character #\# #\\))
