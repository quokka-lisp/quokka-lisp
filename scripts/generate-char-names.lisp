;;;; Writes lisp/library/char-names.lisp: SBCL's name for every Latin-1
;;;; character, codes 0 to 255 (#70). Run from the repo root:
;;;;
;;;;   sbcl --script scripts/generate-char-names.lisp > lisp/library/char-names.lisp
(format t ";;;; The names of the 256 Latin-1 characters, by code (step W, #70).~%")
(format t ";;;;~%;;;; Data, not code: generated once from SBCL — the oracle Quokka is measured~%")
(format t ";;;; against — with scripts/generate-char-names.lisp. The names are Unicode's~%")
(format t ";;;; (`LATIN_SMALL_LETTER_A`), SBCL's for the controls (`Nul`, `Next-Line`), and~%")
(format t ";;;; `U80`-style where Unicode has none. A name is a fact, so this is not copied~%")
(format t ";;;; code. Latin-1, not all of Unicode, because that is where Quokka draws the~%")
(format t ";;;; line for characters elsewhere too: a constant string of Latin-1 characters~%")
(format t ";;;; is packed eight to a word (#44). Unicode's whole table would be hundreds of~%")
(format t ";;;; kilobytes in every image.~%~%")
(format t "(defparameter %latin-1-char-names~%  #(")
(dotimes (code 256)
  (format t "~:[~%    ~;~]~S" (zerop code) (char-name (code-char code))))
(format t "))~%")
