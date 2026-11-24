;;; Step W (#70): the reader's remaining gaps. Tier 1: compiled and SBCL
;;; only. Every change of syntax is made in a copy of the readtable bound
;;; around one form, so the file itself is always read in the standard
;;; syntax.

;; readtable-case: the setting, and that a copy keeps it.
(readtable-case (copy-readtable nil))
(let ((table (copy-readtable nil)))
  (list (setf (readtable-case table) :invert) (readtable-case table)
        (readtable-case (copy-readtable table))))

;; Reading in each case mode: CLHS 23.1.2's table, escapes included.
(mapcar (lambda (mode)
          (let ((*readtable* (copy-readtable nil)))
            (setf (readtable-case *readtable*) mode)
            (mapcar (lambda (text) (symbol-name (read-from-string text)))
                    '("zebra" "Zebra" "ZEBRA" "zebr\\a" "zebr\\A" "ZEBR\\a" "Zebr\\A" "|aB|c"))))
        '(:upcase :downcase :preserve :invert))

;; Numbers keep their syntax whatever the case: exponent markers and radix
;; digits are read without regard to it.
(let ((*readtable* (copy-readtable nil)))
  (setf (readtable-case *readtable*) :preserve)
  (list (read-from-string "1e2") (read-from-string "#x1f") (read-from-string "#\\a")
        (symbol-name (read-from-string "1e2x"))))

;; Printing: the readtable's case and *print-case* together (CLHS 22.1.3.3.2).
(mapcar (lambda (mode)
          (let ((*readtable* (copy-readtable nil)))
            (setf (readtable-case *readtable*) mode)
            (mapcar (lambda (print-case)
                      (let ((*print-case* print-case))
                        (list (mapcar #'prin1-to-string '(|XYZ| |xyz| |Xyz| foo-bar :key))
                              (mapcar #'princ-to-string '(|XYZ| |xyz| |xYZ| x1z)))))
                    '(:upcase :downcase :capitalize))))
        '(:upcase :downcase :preserve :invert))

;; What prints under a case mode reads back as the same symbol.
(mapcar (lambda (mode)
          (let ((*readtable* (copy-readtable nil)))
            (setf (readtable-case *readtable*) mode)
            (every (lambda (sym) (eq sym (read-from-string (prin1-to-string sym))))
                   '(|XYZ| |xyz| |Xyz| foo-bar))))
        '(:upcase :downcase :preserve :invert))

;; read-preserving-whitespace leaves the delimiter in the stream.
(with-input-from-string (in "abc def")
  (list (read-preserving-whitespace in) (read-char in) (read in)))
(with-input-from-string (in "abc def")
  (list (read in) (read-char in)))
(multiple-value-list (read-from-string "abc def" t nil :preserve-whitespace t))
(multiple-value-list (read-from-string "abc def"))
(with-input-from-string (in "") (read-preserving-whitespace in nil :end))

;; Character names: Latin-1, case-insensitive, aliases and U+ codes.
(mapcar #'char-name (list #\Space #\Newline #\a (code-char 0) (code-char 127)
                          (code-char 133) (code-char 160) (code-char 233)))
(mapcar #'char-code
        (mapcar #'name-char '("space" "NEWLINE" "Linefeed" "Lf" "Cr" "Del" "Escape"
                              "latin_small_letter_e_with_acute" "U+3BB" "u3bb")))
(mapcar #'name-char '("U+" "U+110000" "nonesuch" "a"))
(list (name-char 'space) (name-char #\a))
(list (prin1-to-string (code-char 233)) (prin1-to-string #\Tab) (prin1-to-string #\a)
      (read-from-string "#\\LATIN_SMALL_LETTER_E_WITH_ACUTE") (read-from-string "#\\U+41"))
char-code-limit
(format nil "~:C|~:C|~:C|~@C|~@C" #\a (code-char 233) (code-char 133) (code-char 233) #\Space)

;; set-syntax-from-char: whitespace, escapes, macro characters and
;; dispatching ones copied to another character.
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\Space)
  (read-from-string "a!b"))
(let ((*readtable* (copy-readtable nil)))
  (list (set-syntax-from-char #\! #\\)
        (symbol-name (read-from-string "a!bc"))
        (read-from-string "\"a!\"b\"")))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\|)
  (symbol-name (read-from-string "!ab!c")))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\")
  (read-from-string "!ab!"))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\#)
  (read-from-string "!(1 2)"))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\()
  (read-from-string "!1 2)"))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\( #\a)
  (symbol-name (read-from-string "(a")))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\\ #\a)
  (symbol-name (read-from-string "a\\b")))
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\')
  (read-from-string "!x"))

;; The invalid trait: a space made a constituent, and backspace and rubout,
;; may not appear unescaped in a token.
(handler-case (let ((*readtable* (copy-readtable nil)))
                (set-syntax-from-char #\Space #\x)
                (read-from-string "a b"))
  (reader-error () :reader-error))
(handler-case (read-from-string (concatenate 'string "a" (string (code-char 8)) "b"))
  (reader-error () :reader-error))
(handler-case (read-from-string (concatenate 'string "a" (string (code-char 127)) "b"))
  (reader-error () :reader-error))
(length (symbol-name (read-from-string (concatenate 'string "a\\" (string (code-char 8)) "b"))))

;; A copy of a changed readtable carries its syntax with it.
(let ((*readtable* (copy-readtable nil)))
  (set-syntax-from-char #\! #\Space)
  (let ((*readtable* (copy-readtable)))
    (read-from-string "x!y")))

;; Running out of input is an end-of-file condition; a # with no dispatch
;; function is a reader-error.
(mapcar (lambda (text) (handler-case (read-from-string text) (end-of-file () :eof)))
        '("" "(a b" "#(1" "#S(" "  "))
(handler-case (read-from-string "#{") (reader-error () :reader-error))
(with-input-from-string (in "") (handler-case (read-char in) (end-of-file () :eof)))

;; #S calls the structure's constructor.
(defstruct sharp-s-point x y)
(let ((p (read-from-string "#S(sharp-s-point :x 1 :y 2)")))
  (list (sharp-s-point-x p) (sharp-s-point-y p) (sharp-s-point-p p)))
(sharp-s-point-y (read-from-string "#S(SHARP-S-POINT Y 3)"))

;; Negation flips the sign of a zero.
(let ((*read-default-float-format* 'double-float))
  (list (prin1-to-string (- 0.0d0)) (prin1-to-string (- -0.0d0)) (eql -0.0d0 (- 0.0d0))))
