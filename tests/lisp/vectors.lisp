;;; Vectors, strings as sequences, and the functions over both.
;;;
;;; Common Lisp calls a general vector and a string both vectors and both
;;; sequences, so `length`, `aref` and `elt` work on either. Quokka stores
;;; them differently — a vector's slots are values the collector traces, a
;;; string's are raw character codes — and nothing above the primitive table
;;; should be able to tell.

;; Vector literals read and print.
#(1 2 3)
#()
#(a b c)
#(1 (2 3) #(4 5))
'#(1 2 3)

;; Building them.
(vector)
(vector 1 2 3)
(vector 'a "b" #\c)
(make-array 0)
;; `(make-array 3)` with no :initial-element is left out on purpose: CLHS
;; says the contents are undefined until written, and SBCL happens to fill
;; with 0 where Quokka fills with nil. Both conform; neither is testable.
(make-array 3 :initial-element 'x)
(make-array 2 :initial-element 0)

;; Indexing, on vectors and on strings.
(aref #(a b c) 0)
(aref #(a b c) 2)
(aref "abc" 0)
(aref "abc" 2)
(svref #(10 20 30) 1)
(elt '(a b c) 1)
(elt #(a b c) 1)
(elt "abc" 1)

;; Length, on all three kinds of sequence.
(length nil)
(length '(1 2 3))
(length #())
(length #(1 2 3))
(length "")
(length "abcd")

;; Predicates. nil is a list and not a vector; a string is a vector in
;; Common Lisp's sense but `vectorp` of a string is true there too, so
;; Quokka's narrower `vectorp` is tested only on things it agrees about.
(vectorp #(1))
(vectorp '(1))
(vectorp nil)
(stringp "x")
(stringp 'x)
(stringp #(1))
(characterp #\a)
(characterp "a")
(listp nil)
(listp '(1))
(listp #(1))

;; Mutation is NOT tested here. Common Lisp spells it `(setf (aref v i) x)`
;; and Quokka has no `setf` until tier 1, so the only way to write a vector
;; element is `%set-aref` — a name SBCL does not have, which makes the whole
;; expression untestable against the oracle rather than merely different.
;;
;; That is worth knowing about the `%` primitives generally: each one is a
;; piece of behaviour the oracle cannot check. `crates/quokka-vm/src/boot.rs`
;; covers mutation with ordinary Rust tests instead.

;; Two vectors with the same contents are not the same object.
(eq (vector 1 2) (vector 1 2))
(let ((v (vector 1 2))) (eq v v))

;; reverse, on a list and on a vector.
(reverse '(1 2 3))
(reverse #(1 2 3))
(reverse nil)
(reverse #())

;; Characters and string comparison.
(char= #\a #\a)
(char= #\a #\b)
(string= "abc" "abc")
(string= "abc" "abd")
(string= "abc" "ab")
(string= "" "")
(string= "a" "")

;; Vectors hold anything, including other vectors and lists.
(let ((v (vector (list 1 2) (vector 3 4) "five")))
  (list (aref v 0) (aref v 1) (aref v 2)))
(aref (aref #(#(1 2) #(3 4)) 1) 0)

;; Built without mutating: a vector of a computed length, read back.
(defun squares (n)
  (labels ((upto (i acc)
             (if (= i n) (reverse acc) (upto (+ i 1) (cons (* i i) acc)))))
    (apply #'vector (upto 0 nil))))
(squares 0)
(squares 5)
(length (squares 20))
(aref (squares 20) 19)
(elt (squares 20) 0)
