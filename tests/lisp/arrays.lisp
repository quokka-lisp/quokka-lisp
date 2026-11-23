;;; Arrays, CLHS 15 (step T, #67): several dimensions, fill pointers,
;;; adjustable and displaced arrays, bit vectors. Tier 1: compiled and SBCL
;;; only. No array is printed with *print-array* false (SBCL prints its
;;; address), and nothing depends on an element never initialised.

;; make-array: a size or a list of dimensions, initial contents or element.
(make-array 3 :initial-element 'x)
(make-array '(3) :initial-contents '(a b c))
(make-array '(2 3) :initial-contents '((1 2 3) (4 5 6)))
(make-array '(2 2 2) :initial-element 0)
(make-array nil :initial-element 'only)
(make-array '(0 3))
(make-array 4 :element-type 'character :initial-element #\z)
(make-array 3 :element-type 'character :initial-contents "abc")
(make-array 5 :element-type 'bit :initial-element 1)
(make-array '(2 3) :element-type 'bit :initial-contents '((1 0 1) (0 1 0)))

;; aref with several subscripts, and setting through it.
(let ((a (make-array '(2 3) :initial-element 0)))
  (setf (aref a 1 2) 'corner (aref a 0 0) 'origin)
  (list a (aref a 1 2) (aref a 0 1)))
(let ((a (make-array nil :initial-element 1)))
  (setf (aref a) 2)
  (aref a))

;; Dimensions and their questions.
(let ((a (make-array '(2 3 4) :initial-element 0)))
  (list (array-rank a) (array-dimensions a) (array-dimension a 1)
        (array-total-size a) (array-row-major-index a 1 2 3)
        (array-in-bounds-p a 1 2 3) (array-in-bounds-p a 2 0 0)))
(let ((a (make-array '(2 2) :initial-contents '((a b) (c d)))))
  (setf (row-major-aref a 3) 'last)
  (list (row-major-aref a 0) (row-major-aref a 3) a))

;; Fill pointers: length, printing, vector-push and vector-pop.
(let ((v (make-array 5 :fill-pointer 2 :initial-contents '(a b c d e))))
  (list v (length v) (fill-pointer v) (aref v 4) (array-has-fill-pointer-p v)))
(let ((v (make-array 3 :fill-pointer 0)))
  (list (vector-push 'a v) (vector-push 'b v) (vector-push 'c v) (vector-push 'd v)
        v (vector-pop v) v))
(let ((v (make-array 4 :fill-pointer 4 :initial-contents '(1 2 3 4))))
  (setf (fill-pointer v) 1)
  (list v (length v)))
(let ((s (make-array 10 :element-type 'character :fill-pointer 0)))
  (dolist (c '(#\h #\e #\l #\l #\o)) (vector-push c s))
  (list s (length s) (stringp s) (string-upcase s) (reverse s)))

;; Adjustable vectors grow in place.
(let ((v (make-array 2 :fill-pointer 0 :adjustable t)))
  (dotimes (i 10) (vector-push-extend (* i i) v))
  (list v (length v) (>= (array-dimension v 0) 10) (adjustable-array-p v)))
(let ((a (make-array '(2 2) :adjustable t :initial-contents '((1 2) (3 4)))))
  (adjust-array a '(3 3) :initial-element 0)
  a)
(let ((a (make-array 3 :initial-contents '(x y z))))
  (adjust-array a 5 :initial-element 'new))
(let ((a (make-array 4 :adjustable t :initial-contents '(1 2 3 4))))
  (adjust-array a 2)
  a)

;; Displaced arrays share their target's elements.
(let* ((base (make-array 6 :initial-contents '(0 1 2 3 4 5)))
       (window (make-array 3 :displaced-to base :displaced-index-offset 2)))
  (setf (aref window 0) 'changed)
  (list window base
        (multiple-value-bind (to offset) (array-displacement window)
          (list (eq to base) offset))
        (multiple-value-list (array-displacement base))))
(let* ((flat (make-array 6 :initial-contents '(a b c d e f)))
       (grid (make-array '(2 3) :displaced-to flat)))
  (list grid (aref grid 1 0)))

;; Type predicates.
(let ((simple (make-array 3 :initial-element 0))
      (filled (make-array 3 :fill-pointer 1 :initial-element 0))
      (grid (make-array '(2 2) :initial-element 0)))
  (list (mapcar #'arrayp (list simple filled grid "str" #*1 'sym 3))
        (mapcar #'vectorp (list simple filled grid "str" #*1))
        (mapcar #'simple-vector-p (list simple filled grid "str"))
        (mapcar #'simple-string-p (list "str" (make-array 2 :element-type 'character :fill-pointer 1
                                                              :initial-element #\a)))
        (mapcar #'bit-vector-p (list #*1 simple))
        (mapcar #'simple-bit-vector-p (list #*1 (make-array 2 :element-type 'bit
                                                                :fill-pointer 1
                                                                :initial-element 0)))
        (mapcar #'stringp (list "a" (make-array 2 :element-type 'character :fill-pointer 1
                                                  :initial-element #\a)))))
(mapcar #'array-element-type
        (list (make-array 2) (make-array 2 :element-type 'character :initial-element #\a)
              (make-array 2 :element-type 'bit :initial-element 0)))
(mapcar #'upgraded-array-element-type '(t character bit))
(list (adjustable-array-p (make-array 3 :adjustable t))
      (array-has-fill-pointer-p (make-array 3)))

;; Bit vectors: reading, printing, the bit operations.
#*10110
(length #*10110)
(bit #*0101 1)
(let ((b (copy-seq #*0000))) (setf (bit b 2) 1 (sbit b 0) 1) b)
(list (bit-and #*1100 #*1010) (bit-ior #*1100 #*1010) (bit-xor #*1100 #*1010)
      (bit-eqv #*1100 #*1010) (bit-nand #*1100 #*1010) (bit-nor #*1100 #*1010)
      (bit-andc1 #*1100 #*1010) (bit-andc2 #*1100 #*1010) (bit-orc1 #*1100 #*1010)
      (bit-orc2 #*1100 #*1010) (bit-not #*1100))
(let ((into (copy-seq #*1111)))
  (bit-and #*1010 #*0110 into)
  into)
(bit-and (make-array '(2 2) :element-type 'bit :initial-contents '((1 1) (0 1)))
         (make-array '(2 2) :element-type 'bit :initial-contents '((1 0) (1 1))))
(list (equal #*101 #*101) (equal #*101 #*100) (equalp #*1 #*1))
(list (remove 1 #*10101) (reverse #*1100) (count 1 #*10111) (subseq #*101100 2 5))
(coerce '(1 0 1) 'bit-vector)

;; Arrays of several dimensions read and print as #nA.
#2A((1 2) (3 4))
#0Aseven
(equalp #2A((1 2) (3 4)) (make-array '(2 2) :initial-contents '((1 2) (3 4))))
(let ((*print-length* 2)) (prin1-to-string (make-array '(3 3) :initial-element 0)))
(let ((*print-level* 1)) (prin1-to-string (make-array '(2 2) :initial-element 0)))

;; Strings with fill pointers work as strings elsewhere too.
(let ((s (make-array 6 :element-type 'character :fill-pointer 3 :initial-contents "FOOBAR")))
  (list (string= s "FOO") (equal s "FOO") (intern s "KEYWORD") (format nil "~A" s)))
(let ((h (make-hash-table :test 'equal))
      (s (make-array 4 :element-type 'character :fill-pointer 3 :initial-contents "keyX")))
  (setf (gethash "key" h) 'found)
  (gethash s h))

;; Errors.
(handler-case (aref (make-array '(2 2) :initial-element 0) 2 0) (error () :out-of-bounds))
(handler-case (aref (make-array '(2 2) :initial-element 0) 0) (error () :wrong-rank))
(handler-case (vector-pop (make-array 2 :fill-pointer 0)) (error () :empty))
(handler-case (fill-pointer (make-array 2)) (error () :no-fill-pointer))
(handler-case (setf (aref (copy-seq #*00) 0) 2) (error () :not-a-bit))
(list (>= array-rank-limit 8) (> array-dimension-limit 1000) (> array-total-size-limit 1000))

;; A start or end that does not fit is a type-error, as SBCL signals.
(list (handler-case (fill (make-array 5) 'x :start -1) (type-error () :type-error))
      (handler-case (fill (make-array 5) 'x :end 'a) (type-error () :type-error))
      (handler-case (subseq "abc" 2 5) (type-error () :type-error)))

;; An array displaced onto one that has since shrunk points past its end:
;; reading it is an error, not a crash of the machine.
(let* ((base (make-array 6 :adjustable t :initial-element 0))
       (window (make-array 3 :displaced-to base :displaced-index-offset 3)))
  (adjust-array base 2)
  (handler-case (aref window 2) (error () :error)))
