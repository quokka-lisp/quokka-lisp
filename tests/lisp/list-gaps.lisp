;;; Dotted and circular lists, equalp hash tables, defpackage's checks,
;;; strings that are not simple as names, values places (step AF, #79).
;;; Tier 1: compiled and SBCL only.

;; last, butlast and nbutlast count conses, so a dotted list works.
(last '(a b . c))
(last '(a . b) 0)
(last '(a . b) 1)
(last '(a . b) 2)
(last '(a b c . d) 2)
(butlast '(a b c . d) 1)
(butlast '(a b c . d) 3)
(let ((x (list* 'a 'b 'c 'd))) (let ((r (nbutlast x 1))) (list (eq r x) r)))

;; nconc never walks its last argument; an atom before it is skipped.
(let ((x (list 'a 'b 'c))) (nconc x x) (list (eq (cdddr x) x) (list-length x)))
(nconc (list 'a) (list 'b) 'foo)
(nconc (copy-tree '(a . b)) (copy-tree '(c . d)) (copy-tree '(e . f)) 'foo)
(nconc nil 'x)
(mapcan (lambda (x) x 1) (list 'a))
(mapcon (lambda (x) x 1) (list 'a))
(mapcan (lambda (x) (list x x)) '(1 2 3))

;; tree-equal is true or false, one value.
(multiple-value-list (tree-equal 1 2 :test (lambda (x y) (values (< x y) t))))

;; equalp of hash tables: the same count, the same test, equalp values.
(equalp (make-hash-table :test 'eq) (make-hash-table :test #'eq))
(equalp (make-hash-table :test 'eq) (make-hash-table :test 'eql))
(let ((a (make-hash-table)) (b (make-hash-table)))
  (setf (gethash 'k a) "A" (gethash 'k b) "a")
  (equalp a b))
(let ((a (make-hash-table)) (b (make-hash-table)))
  (setf (gethash 'k a) 1 (gethash 'j b) 1)
  (equalp a b))
(let ((a (make-hash-table :test 'equal)) (b (make-hash-table :test 'equal)))
  (setf (gethash #\a a) "a" (gethash #\a b) "A" (gethash "x" a) 2)
  (equalp a b))

;; A string with a fill pointer names a symbol by its active characters.
(symbol-name (make-symbol (make-array 6 :element-type 'character
                                        :initial-contents "ABCDEF" :fill-pointer 4)))
(eq (intern (make-array 5 :element-type 'character :initial-contents "XYZZY"))
    (intern "XYZZY"))

;; defpackage refuses options that contradict each other.
(handler-case (defpackage "LG-A" (:use) (:size 10) (:size 20)) (program-error () :refused))
(handler-case (defpackage "LG-B" (:use) (:documentation "x") (:documentation "y"))
  (program-error () :refused))
(handler-case (defpackage "LG-C" (:use) (:export "A") (:intern "A")) (program-error () :refused))
(handler-case (defpackage "LG-D" (:use) (:shadow "A") (:intern "A")) (program-error () :refused))
(find-package "LG-C")

;; A nested values place gets one value; psetf takes several.
(let ((a t) (b t) (c t) (d t) (e t) (f t))
  (setf (values a (values b c) (values d) (values e f)) (values 0 1 2 3 4 5 6))
  (list a b c d e f))
(let (a b c d e f)
  (multiple-value-list
   (setf (values a (values b c) (values d) (values e f)) (values 0 1 2 3 4 5 6))))
(let ((y 2) (z 3) u x a b c)
  (psetf (values a b c) (values 1 2 3) (values u x) (values y z))
  (list a b c u x))
(let ((a 1) (b 2)) (psetf a b b a) (list a b))
