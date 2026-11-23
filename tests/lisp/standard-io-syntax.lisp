;;; with-standard-io-syntax, the standard I/O variables, *read-base*,
;;; *print-readably* and *print-circle* (step Q, #64). Tier 1: compiled and
;;; SBCL only.

;; Every variable at its standard value inside, whatever it was outside.
(let ((*print-base* 8) (*print-case* :downcase) (*print-escape* nil)
      (*print-length* 3) (*print-level* 2) (*print-radix* t) (*read-base* 16)
      (*read-eval* nil) (*print-gensym* nil))
  (with-standard-io-syntax
    (list *print-array* *print-base* *print-case* *print-circle* *print-escape*
          *print-gensym* *print-length* *print-level* *print-lines*
          *print-miser-width* *print-pretty* *print-radix* *print-readably*
          *print-right-margin* *read-base* *read-default-float-format*
          *read-eval* *read-suppress* (package-name *package*)
          (readtablep *readtable*))))

;; It is a progn: no forms is nil, several values come back, go and return
;; pass through it.
(with-standard-io-syntax)
(multiple-value-list (with-standard-io-syntax (values 'a 'b 'c)))
(block done
  (tagbody (with-standard-io-syntax (go out)) (return-from done :bad)
   out (return-from done :good)))

;; Printing inside: readably, in base 10, upper case, with escapes.
(let ((*print-base* 2) (*print-case* :downcase))
  (with-standard-io-syntax (prin1-to-string '(abc "x" 10))))

;; *read-base*: digits in the base; a trailing point means decimal; a token
;; that is not digits in the base is a symbol.
(let ((*read-base* 16))
  (list (read-from-string "ff") (read-from-string "-a") (read-from-string "10.")
        (read-from-string "1e5") (read-from-string "1.5") (read-from-string "fg")))
(let ((*read-base* 2)) (list (read-from-string "101") (read-from-string "102")))
(let ((*read-base* 36)) (read-from-string "zz"))

;; The printer escapes by *print-base*, not *read-base*.
(let ((*read-base* 16)) (prin1-to-string '|FF|))
(let ((*print-base* 16)) (prin1-to-string '|FF|))

;; *print-readably*: escapes on, length and level off.
(write-to-string '(1 2 3 4 5) :readably t :length 2)
(write-to-string '((((deep)))) :readably t :level 1)
(write-to-string "s" :readably t :escape nil)
(write-to-string (make-symbol "G") :readably t :gensym nil)
(princ-to-string "plain")

;; An object nothing can read back: an error, of type print-not-readable.
(handler-case (write-to-string #'car :readably t)
  (print-not-readable () :not-readable))
(handler-case (with-standard-io-syntax (prin1-to-string (find-package "CL")))
  (print-not-readable () :not-readable))

;; write takes every keyword, and :stream.
(with-output-to-string (s) (write 12 :stream s :base 2 :radix t :readably nil))
(with-output-to-string (s) (write '(a b c) :stream s :length 1 :pretty nil :circle nil))

;; *print-circle*: a shared list, a shared tail, a cycle, shared strings and
;; gensyms; equal but distinct objects are not labelled.
(let ((x (list 1 2)))
  (write-to-string (list x x (list 1 2)) :circle t))
(let ((tail (list 'c 'd)))
  (write-to-string (list (cons 'a tail) (cons 'b tail)) :circle t))
(let ((cycle (list 1 2 3)))
  (setf (cdr (cddr cycle)) cycle)
  (write-to-string cycle :circle t))
(let ((self (list 'a 'b)))
  (setf (car (cdr self)) self)
  (write-to-string self :circle t))
(let ((s "ab") (g (make-symbol "G")))
  (write-to-string (list s s g g "ab" 'sym 'sym 1 1) :circle t))
(let ((v (vector 1 2)))
  (write-to-string (vector v v) :circle t))
(let ((x (list 'shared)))
  (write-to-string (list x x) :circle nil))
(let ((x (list 1)))
  (write-to-string (list (list x x) x (list x)) :circle t))
(let ((v (vector 'a nil)))
  (setf (aref v 1) v)
  (write-to-string v :circle t))
