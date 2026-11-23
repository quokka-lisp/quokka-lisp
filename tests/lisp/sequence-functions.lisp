;;;; The sequence functions of CLHS chapter 17, on lists, vectors and
;;;; strings (#54, step J2).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;; Nothing here depends on what CL leaves open: `sort` is only given
;;;; distinct keys (only `stable-sort` fixes the order of equal ones), and no
;;;; test looks at what a destructive function left in its argument.

;; Making sequences.
(list (make-sequence 'list 3 :initial-element 'x) (make-sequence 'vector 2 :initial-element 0)
      (make-sequence 'string 3 :initial-element #\z))
(list (concatenate 'list '(1 2) #(3 4) "ab") (concatenate 'string "ab" '(#\c) #(#\d))
      (concatenate 'vector '(1) #(2)))
(list (coerce '(1 2) 'vector) (coerce #(1 2) 'list) (coerce '(#\a #\b) 'string) (coerce "ab" 'list))

;; Reading.
(list (elt '(a b c) 1) (elt #(a b c) 2) (elt "abc" 0))
(handler-case (elt '(a b) 5) (error () 'out-of-range))
(list (subseq '(1 2 3 4) 1 3) (subseq #(1 2 3 4) 2) (subseq "hello" 1 4) (subseq '(1 2) 2))
(handler-case (subseq '(1 2) 1 5) (error () 'bad-range))
(let ((s (copy-seq "hello"))) (setf (subseq s 1 3) "EY") s)
(let ((l (list 1 2 3 4))) (setf (subseq l 1) '(b c)) l)
(list (nreverse (list 1 2 3)) (nreverse (vector 1 2 3)) (reverse "abc"))

;; find, position, count — and their -if and -if-not.
(list (find 3 '(1 2 3 4)) (find 9 '(1 2 3)) (find #\b "abc") (find 2 #(1 2 3)))
(list (find 2 '((1 a) (2 b)) :key #'car) (find "b" '("a" "b") :test #'equal)
      (find 2 '(1 2 3 4) :test #'<) (find 2 '(1 2 3) :test-not #'eql))
(list (find-if #'evenp '(1 3 4 6)) (find-if #'evenp '(1 3 4 6) :from-end t)
      (find-if-not #'oddp '(1 3 4)) (find-if #'evenp '(2 4 6) :start 1 :end 2))
(list (position 3 '(1 2 3 4)) (position #\c "abcabc") (position #\c "abcabc" :from-end t)
      (position 1 #(1 2 1) :start 1) (position 9 '(1 2)))
(list (position-if #'evenp '(1 3 4 5 6) :from-end t) (position-if-not #'oddp #(1 3 4)))
(list (count 1 '(1 2 1 3 1)) (count #\a "banana") (count-if #'evenp #(1 2 4 5))
      (count-if-not #'evenp '(1 2 3)) (count 1 '(1 1 1) :start 1))

;; remove, delete, substitute — :count and :from-end decide which ones.
(list (remove 1 '(1 2 1 3 1)) (remove 1 '(1 2 1 3 1) :count 2)
      (remove 1 '(1 2 1 3 1) :count 2 :from-end t) (remove 1 '(1 2 1 3 1) :start 1 :end 3))
(list (remove #\a "banana") (remove 2 #(1 2 3 2)) (remove-if #'evenp '(1 2 3 4))
      (remove-if-not #'evenp #(1 2 3 4)) (remove 3 '((1) (3) (5)) :key #'car))
(list (delete 1 (list 1 2 1)) (delete-if #'oddp (vector 1 2 3)) (delete-if-not #'oddp (list 1 2 3)))
(list (substitute 'x 1 '(1 2 1)) (substitute #\o #\a "banana" :count 2)
      (substitute 0 1 #(1 2 1) :from-end t :count 1) (substitute-if 0 #'evenp '(1 2 3 4))
      (substitute-if-not 0 #'evenp '(1 2 3 4)))
(list (nsubstitute 'x 1 (list 1 2)) (nsubstitute-if 0 #'oddp (vector 1 2)) (nsubstitute-if-not 0 #'oddp (list 1 2)))
(list (remove-duplicates '(1 2 1 3 2)) (remove-duplicates '(1 2 1 3 2) :from-end t)
      (remove-duplicates "abracadabra") (remove-duplicates '((a 1) (b 2) (a 3)) :key #'car)
      (remove-duplicates '("a" "A" "b") :test #'equalp) (delete-duplicates (list 1 1 2)))

;; Writing into a sequence.
(list (fill (list 1 2 3) 0) (fill (vector 1 2 3 4) 'x :start 1 :end 3) (fill (make-string 3) #\q))
(list (replace (list 1 2 3 4) '(a b) :start1 1) (replace (vector 1 2 3) #(x y z w))
      (replace (copy-seq "abcdef") "XY" :start1 2 :start2 0 :end2 1))
(let ((v (vector 1 2 3 4 5))) (replace v v :start1 1) v)
(let ((r (list 0 0 0))) (map-into r #'+ '(1 2 3) '(10 20)) r)

;; map, the quantifiers, reduce.
(list (map 'list #'+ '(1 2 3) #(10 20)) (map 'vector #'char-upcase "abc") (map 'string #'char-upcase '(#\x #\y))
      (map nil #'identity '(1 2)))
(list (some #'evenp '(1 3 4)) (some #'evenp '(1 3)) (some (lambda (x) (and (> x 1) (* x 10))) #(1 2 3))
      (every #'evenp '(2 4)) (every #'evenp '(2 3)) (every #'< '(1 2) '(2 3 0)))
(list (notany #'evenp '(1 3)) (notany #'evenp '(1 2)) (notevery #'evenp '(2 4)) (notevery #'evenp '(2 3)))
(list (reduce #'+ '(1 2 3 4)) (reduce #'list '(1 2 3)) (reduce #'list '(1 2 3) :from-end t)
      (reduce #'+ #(1 2 3) :initial-value 10) (reduce #'list '(1 2) :initial-value 0 :from-end t))
(list (reduce #'+ nil) (reduce #'+ '(7)) (reduce #'+ '((1) (2)) :key #'car) (reduce #'+ '(1 2 3 4) :start 1 :end 3))

;; search and mismatch.
(list (search "na" "banana") (search "na" "banana" :from-end t) (search '(2 3) '(1 2 3 4)) (search "x" "abc")
      (search "" "abc") (search "AN" "banana" :test #'equalp))
(list (mismatch "abcd" "abxd") (mismatch "abc" "abc") (mismatch "ab" "abc") (mismatch '(1 2 3) #(1 2 4))
      (mismatch "abcd" "xbcd" :from-end t) (mismatch "bcd" "abcd" :from-end t))

;; Sorting: merge sort, stable.
(list (sort (list 3 1 2) #'<) (sort (vector 5 4 6) #'<) (sort (copy-seq "banana") (lambda (a b) (< (char-code a) (char-code b)))))
(stable-sort (list '(1 a) '(0 b) '(1 c) '(0 d)) #'< :key #'car)
(stable-sort (vector "bb" "a" "cc" "d") #'< :key #'length)
(list (merge 'list (list 1 3 5) (list 2 4) #'<) (merge 'vector #(1 4) '(2 3) #'<)
      (merge 'list (list '(1 a)) (list '(1 b)) #'< :key #'car))
(sort (list 5 2 8 1 9 3 7 4 6 0) #'>)
