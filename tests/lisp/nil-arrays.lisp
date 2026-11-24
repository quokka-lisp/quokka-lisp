;;; Arrays of element type nil (step AD, #77): an array that can hold no
;;; element, since no object is of type nil. CLHS 15.1.2.2 calls (vector nil)
;;; a string; SBCL does not, and Quokka follows SBCL. Tier 1: compiled and
;;; SBCL only. None is printed (SBCL prints its address), and nothing here
;;; reads or writes an element, which is an error whose message differs.

(stringp (make-array 0 :element-type nil))
(stringp (make-array 37 :element-type nil))
(simple-string-p (make-array 0 :element-type nil))
(simple-string-p (make-array 5 :element-type nil :fill-pointer 2))
(stringp (make-array 5 :element-type nil :fill-pointer 2))
(vectorp (make-array 3 :element-type nil))
(length (make-array 37 :element-type nil))
(length (make-array 5 :element-type nil :fill-pointer 2))
(array-element-type (make-array 3 :element-type nil))
(array-element-type (make-array 3 :element-type nil :adjustable t))
(upgraded-array-element-type nil)
(array-dimensions (make-array '(2 3) :element-type nil))

;; Not a string, so not a string designator either.
(string-upcase (make-array 0 :element-type nil))
(string-trim "ab" (make-array 0 :element-type nil))
(string-trim (make-array 0 :element-type nil) "abc")
(string= (make-array 0 :element-type nil) "")
(string< "" (make-array 0 :element-type nil))
(equal "" (make-array 0 :element-type nil))
(length (string (make-array 0 :element-type nil)))

;; Reading or writing an element is an error.
(handler-case (aref (make-array 3 :element-type nil) 0)
  (error () :error))
(handler-case (setf (aref (make-array 3 :element-type nil) 0) #\a)
  (error () :error))
