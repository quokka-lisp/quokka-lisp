;;; Step AA (#74): the sequence functions' plain failures. `count` with
;;; `:from-end`, a result type's length, `map-into` with a fill pointer,
;;; `nsubstitute` writing in place, `make-sequence` of a string. Tier 1:
;;; compiled and SBCL only.

;; :from-end changes the order the key sees the elements in.
(let ((c 0))
  (count 1 '(1 2 3 7 4 5 7 6 2 8) :from-end t :key (lambda (x) (prog1 (- x c) (incf c)))))
(let ((seen nil)) (count-if #'oddp #(1 2 3) :from-end t :key (lambda (x) (push x seen) x)) seen)
(let ((seen nil)) (count-if-not #'oddp '(1 2 3) :from-end t :key (lambda (x) (push x seen) x)) seen)

;; nsubstitute changes the sequence it is given.
(let ((x (list 'a 'b 'a 'c))) (nsubstitute 'b 'a x) x)
(let ((x (vector 1 2 1))) (nsubstitute-if 0 #'oddp x :count 1 :from-end t) x)
(let ((x (copy-seq "banana"))) (nsubstitute-if-not #\o (lambda (c) (char= c #\a)) x :start 1) x)
(let* ((base (vector 1 2 3 4))
       (window (make-array 2 :displaced-to base :displaced-index-offset 1)))
  (nsubstitute 9 3 window)
  base)

;; A result type that fixes a length must get that many elements.
(handler-case (make-sequence '(vector * 4) 3) (type-error () :type-error))
(handler-case (make-sequence 'cons 0) (type-error () :type-error))
(handler-case (make-sequence 'null 1) (type-error () :type-error))
(handler-case (concatenate '(vector * 3) '(a b c d e)) (type-error () :type-error))
(handler-case (map '(vector * 8) #'identity '(a b c)) (type-error () :type-error))
(handler-case (merge 'null (list 1 2 3) (list 4 5 6) #'<) (type-error () :type-error))
(handler-case (merge '(bit-vector 3) (list 0 0 0) (list 1 1 1) #'<) (type-error () :type-error))
(concatenate '(vector * 3) '(a) '(b c))
(make-sequence '(simple-array t (2)) 2 :initial-element 'z)
(make-sequence 'null 0)
(coerce '(1 2) '(vector t 2))

;; Without an initial element a string is made of characters.
(length (make-sequence 'string 10))
(length (make-sequence '(vector character) 3))
(make-sequence 'bit-vector 3)

;; map-into fills a vector up to its dimension and sets its fill pointer.
(let ((a (make-array 6 :fill-pointer 3)))
  (list (map-into a #'identity '(1 2 3 4 5)) (fill-pointer a)))
(let ((s (make-array 5 :element-type 'character :fill-pointer 0)))
  (list (map-into s #'char-upcase "abc") (fill-pointer s)))
(map-into (list 1 2 3) #'+ '(10 20) '(100 200 300))
