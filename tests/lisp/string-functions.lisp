;;;; Characters and strings: the essentials of CLHS chapters 13 and 16
;;;; (#54, step J3).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;; Case-insensitive comparisons are only asked about letters and digits:
;;;; how a letter compares with `[` or `_` is left to the implementation.

;; Character comparisons, any number of arguments.
(list (char= #\a #\a #\a) (char= #\a #\b) (char/= #\a #\b #\c) (char/= #\a #\b #\a))
(list (char< #\a #\b #\c) (char< #\a #\c #\b) (char> #\c #\b) (char<= #\a #\a #\b) (char>= #\b #\b #\a))
(list (char-equal #\a #\A) (char-not-equal #\a #\B) (char-lessp #\a #\B) (char-greaterp #\Z #\a)
      (char-not-greaterp #\a #\A #\b) (char-not-lessp #\b #\B #\a))
(list (character #\x) (character "y") (character 'z))

;; Strings as designators, and char/schar.
(list (string "abc") (string 'hello) (string #\q))
(list (char "hello" 1) (schar "hello" 4))
(let ((s (copy-seq "cat"))) (setf (char s 0) #\b) (setf (schar s 2) #\d) s)
(list (make-string 2 :initial-element #\x) (length (make-string 3 :element-type 'character)))

;; Comparisons: t or nil for equality, otherwise where the strings differ.
(list (string= "abc" "abc") (string= "abc" "abd") (string= "ab" "abc") (string= 'abc "ABC"))
(list (string= "xabcx" "abc" :start1 1 :end1 4) (string= "abc" "zabz" :start2 1 :end2 3 :end1 2))
(list (string/= "abc" "abd") (string/= "abc" "abc") (string< "abc" "abd") (string< "ab" "abc")
      (string< "abc" "ab") (string> "abd" "abc") (string<= "abc" "abc") (string>= "b" "abc"))
(list (string-equal "Hello" "hELLO") (string-not-equal "abc" "ABD") (string-lessp "apple" "Banana")
      (string-greaterp "b" "A") (string-not-greaterp "ABC" "abc") (string-not-lessp "abd" "ABC"))
(list (string< "" "a") (string< "a" "") (string= "" ""))

;; Changing case.
(list (string-upcase "hello World") (string-downcase "Hello WORLD") (string-upcase "hello" :start 1 :end 3))
(list (string-capitalize "hello wORLD foo-bar 3rd") (string-capitalize 'symbol-name))
(let ((s (copy-seq "abc"))) (list (nstring-upcase s) s))
(let ((s (copy-seq "ABC"))) (nstring-downcase s :start 1) s)
(let ((s (copy-seq "two words"))) (nstring-capitalize s))

;; Trimming.
(list (string-trim " " "  padded  ") (string-left-trim " " "  padded  ") (string-right-trim " " "  padded  "))
(list (string-trim '(#\x #\y) "xyhixy") (string-trim "abc" "abc") (string-trim "" "keep"))

;; Strings are sequences: J2's functions on them.
(list (position #\l "hello") (count #\l "hello") (remove #\l "hello") (reverse "hello") (subseq "hello" 1 3))
(list (search "lo" "hello") (sort (copy-seq "hello") #'char<) (find #\L "hello" :test #'char-equal))
(list (equal "abc" "abc") (equalp "abc" "ABC") (equal "abc" "ABC"))
