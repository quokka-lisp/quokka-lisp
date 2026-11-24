;;;; Arrays (#67, step T): CLHS chapter 15, pulled forward from tier 2.
;;;;
;;;; A *simple* array is what Quokka always had: a general vector, a string,
;;;; and now a bit vector, each one-dimensional with nothing else to it. Every
;;;; other array — several dimensions, a fill pointer, adjustable, displaced
;;;; onto another array — is a *header* in front of the array holding its
;;;; elements in row-major order. Rust knows the header's layout and follows
;;;; it in `aref`, `%set-aref` and `length` (`crates/quokka-vm/src/array.rs`,
;;;; decision 2); everything here is Lisp on top of that.
;;;;
;;;; A header's slots, as `array.rs` numbers them: 0 the data, 1 the offset
;;;; of this array's first element in it, 2 the fill pointer or nil, 3 the
;;;; dimensions, 4 whether it is displaced, 5 whether it is adjustable.

(defun %header-p (object) (eql (%array-kind object) 4))
(defun %data (array) (%array-header-slot array 0))
(defun %offset (array) (%array-header-slot array 1))
(defun %fill (array) (%array-header-slot array 2))
(defun %dimensions (array) (%array-header-slot array 3))

;;; ------------------------------------------------------------------------
;;; What kind of array

(defun arrayp (object) (not (eql (%array-kind object) 0)))
(defun simple-vector-p (object) (eql (%array-kind object) 1))
(defun simple-string-p (object) (eql (%array-kind object) 2))
(defun simple-bit-vector-p (object) (eql (%array-kind object) 3))
(defun bit-vector-p (object) (and (vectorp object) (eql (%array-kind (%storage object)) 3)))

(defun %check-array (object)
  (if (not (arrayp object)) (error 'type-error "not an array" object))
  object)

;;; The simple array an array's elements are in, and the index there of its
;;; first: a header's data, followed through any displacement.
(defun %storage (array)
  (let ((start 0))
    (do () ((not (%header-p array)) (values array start))
      (setq start (+ start (%offset array)))
      (setq array (%data array)))))

;;; The three kinds of element an array can be specialised to: CLHS lets an
;;; implementation *upgrade* any other element type to `t` (15.1.2.1), as
;;; SBCL does for most.
(defun %element-kind (type)
  (cond ((member type '(character base-char standard-char extended-char)) :character)
        ((or (eq type 'bit) (equal type '(unsigned-byte 1)) (equal type '(integer 0 1))
             (equal type '(mod 2)))
         :bit)
        (t :t)))

(defun upgraded-array-element-type (type &optional environment)
  environment
  (let ((kind (%element-kind type)))
    (cond ((eq kind :character) 'character) ((eq kind :bit) 'bit) (t t))))

(defun array-element-type (array)
  (let ((kind (%array-kind (%storage (%check-array array)))))
    (cond ((eql kind 2) 'character) ((eql kind 3) 'bit) (t t))))

;;; ------------------------------------------------------------------------
;;; Dimensions

;;; The limits CLHS asks every implementation to state. An image is the same
;;; on every target, so these are below what wasm32's 32-bit heap can hold
;;; rather than what a 64-bit one could, and the rank is bounded only so
;;; that `aref` stays a short primitive (rule 9).
(defconstant array-rank-limit 1024)
(defconstant array-dimension-limit 2147483647)
(defconstant array-total-size-limit 2147483647)

(defun array-dimensions (array)
  (if (%header-p (%check-array array))
      (copy-list (%dimensions array))
      (list (%sequence-length array))))

(defun array-rank (array) (length (array-dimensions array)))
(defun array-dimension (array axis) (nth axis (array-dimensions array)))

(defun %product (numbers)
  (let ((n 1)) (dolist (each numbers n) (setq n (* n each)))))

(defun array-total-size (array) (%product (array-dimensions array)))

(defun array-in-bounds-p (array &rest subscripts)
  (let ((dimensions (array-dimensions array)))
    (if (not (= (length subscripts) (length dimensions)))
        (error 'program-error "one subscript for each dimension"))
    (every (lambda (i d) (and (integerp i) (<= 0 i) (< i d))) subscripts dimensions)))

(defun array-row-major-index (array &rest subscripts)
  (if (not (apply #'array-in-bounds-p array subscripts))
      (error "subscripts out of the array's bounds" subscripts))
  (let ((index 0))
    (mapc (lambda (i d) (setq index (+ (* index d) i))) subscripts (array-dimensions array))
    index))

(defun row-major-aref (array index)
  (if (not (and (integerp index) (<= 0 index) (< index (array-total-size array))))
      (error "row-major index out of the array's bounds" index))
  (multiple-value-bind (simple start) (%storage array)
    (aref simple (+ start index))))

(defun %set-row-major-aref (array index value)
  (if (not (and (integerp index) (<= 0 index) (< index (array-total-size array))))
      (error "row-major index out of the array's bounds" index))
  (multiple-value-bind (simple start) (%storage array)
    (setf (aref simple (+ start index)) value)))

(defsetf row-major-aref %set-row-major-aref)

;;; ------------------------------------------------------------------------
;;; Making arrays

(defun %dimension-list (dimensions)
  (let ((dimensions (if (listp dimensions) dimensions (list dimensions))))
    (if (>= (length dimensions) array-rank-limit)
        (error "more dimensions than array-rank-limit" dimensions))
    (dolist (d dimensions dimensions)
      (if (not (and (integerp d) (>= d 0) (< d array-dimension-limit)))
          (error 'type-error "an array dimension is a non-negative integer" d)))))

;;; A fresh simple array of N elements of KIND, each INITIAL if one is given.
(defun %make-simple (kind n initial initial-p)
  (cond ((eq kind :character)
         (make-string n :initial-element (if initial-p initial (code-char 0))))
        ((eq kind :bit)
         (let ((bits (%make-bit-vector n)))
           (if (and initial-p (not (eql initial 0)))
               (dotimes (i n) (setf (aref bits i) initial)))
           bits))
        (t (%make-vector n initial))))

;;; Fill SIMPLE, in row-major order, from CONTENTS: nested sequences as deep
;;; as there are DIMENSIONS, each as long as its dimension.
(defun %fill-contents (simple dimensions contents)
  (let ((at 0))
    (labels ((fill-level (dimensions contents)
               (if (null dimensions)
                   (progn (setf (aref simple at) contents)
                          (setq at (+ at 1)))
                   (progn
                     (if (not (eql (length contents) (car dimensions)))
                         (error "initial contents do not match the dimensions" contents))
                     (map nil (lambda (each) (fill-level (cdr dimensions) each)) contents)))))
      (fill-level dimensions contents))
    simple))

(defun %fill-pointer-value (fill-pointer size)
  (cond ((null fill-pointer) nil)
        ((eq fill-pointer t) size)
        ((and (integerp fill-pointer) (<= 0 fill-pointer size)) fill-pointer)
        (t (error "a fill pointer is between 0 and the vector's size" fill-pointer))))

;;; `(make-array dimensions &key ...)`, all of CLHS's keywords. An array
;;; with one dimension and no fill pointer, adjustability or displacement is
;;; simple and is the data itself; any other gets a header.
;;;
;;; The compiler and the sequence functions make plain vectors all the time,
;;; and parsing eight keywords cost twelve times what making the vector did:
;;; loading ansi-test's runner went over its step budget. So a size with no
;;; keywords goes straight to the primitive, as the prelude's did.
(defun make-array (dimensions &rest options)
  (if (and (null options) (integerp dimensions) (>= dimensions 0))
      (%make-vector dimensions nil)
      (apply #'%make-array dimensions options)))

(defun %make-array (dimensions &key (element-type t)
                                   (initial-element nil initial-element-p)
                                   (initial-contents nil initial-contents-p)
                                   adjustable fill-pointer displaced-to
                                   (displaced-index-offset 0))
  (let* ((dimensions (%dimension-list dimensions))
         (size (%product dimensions)))
    (if (and fill-pointer (not (eql (length dimensions) 1)))
        (error "only a vector can have a fill pointer"))
    (if (and initial-element-p initial-contents-p)
        (error "make-array takes initial-element or initial-contents, not both"))
    (if (and displaced-to (> (+ displaced-index-offset size) (array-total-size displaced-to)))
        (error "a displaced array must fit inside the one it is displaced to"))
    (let ((data (if displaced-to
                    displaced-to
                    (let ((simple (%make-simple (%element-kind element-type) size
                                                initial-element initial-element-p)))
                      (if initial-contents-p
                          (%fill-contents simple dimensions initial-contents)
                          simple)))))
      (if (and (eql (length dimensions) 1) (not adjustable) (not fill-pointer) (not displaced-to))
          data
          (%make-array-header data (if displaced-to displaced-index-offset 0)
                              (%fill-pointer-value fill-pointer size)
                              dimensions (if displaced-to t nil) (if adjustable t nil))))))

;;; ------------------------------------------------------------------------
;;; Header properties

(defun adjustable-array-p (array)
  (and (%header-p (%check-array array)) (%array-header-slot array 5) t))

(defun array-has-fill-pointer-p (array)
  (and (%header-p (%check-array array)) (%fill array) t))

(defun array-displacement (array)
  (if (and (%header-p (%check-array array)) (%array-header-slot array 4))
      (values (%data array) (%offset array))
      (values nil 0)))

;;; ------------------------------------------------------------------------
;;; Fill pointers

(defun fill-pointer (vector)
  (if (not (array-has-fill-pointer-p vector))
      (error 'type-error "a vector with a fill pointer" vector))
  (%fill vector))

(defun %set-fill (vector index) (%set-array-header-slot vector 2 index))

(defun %set-fill-pointer (vector index)
  (fill-pointer vector)
  (%set-fill vector (%fill-pointer-value index (array-dimension vector 0))))

(defsetf fill-pointer %set-fill-pointer)

;;; Store OBJECT at the fill pointer and advance it; nil if the vector is
;;; full, and the index stored at otherwise.
(defun vector-push (object vector)
  (let ((index (fill-pointer vector)))
    (if (< index (array-dimension vector 0))
        (progn (setf (aref vector index) object)
               (%set-fill vector (+ index 1))
               index)
        nil)))

;;; The same, growing the vector when it is full. CLHS leaves how much to
;;; grow by to the implementation, beyond EXTENSION; doubling keeps a long
;;; run of pushes linear.
(defun vector-push-extend (object vector &optional extension)
  (let ((index (fill-pointer vector))
        (size (array-dimension vector 0)))
    (if (>= index size)
        (progn
          (if (not (adjustable-array-p vector))
              (error "vector-push-extend needs an adjustable vector" vector))
          (adjust-array vector (+ size (max 1 size (if extension extension 0))))))
    (vector-push object vector)))

(defun vector-pop (vector)
  (let ((index (fill-pointer vector)))
    (if (eql index 0) (error "vector-pop of an empty vector" vector))
    (%set-fill vector (- index 1))
    (aref vector (- index 1))))

;;; ------------------------------------------------------------------------
;;; Adjusting

;;; The subscripts of row-major INDEX in an array of DIMENSIONS.
(defun %subscripts (index dimensions)
  (let ((subscripts nil))
    (dolist (d (reverse dimensions) subscripts)
      (push (rem index d) subscripts)
      (setq index (truncate index d)))))

;;; `(adjust-array array dimensions &key ...)` — an adjustable array is
;;; changed in place, so every reference to it sees the change: its header
;;; gets new data. Any other array is copied into a new one. Elements whose
;;; subscripts are inside both the old and the new dimensions keep their
;;; values (CLHS).
(defun adjust-array (array dimensions &key (element-type (array-element-type array))
                                          (initial-element nil initial-element-p)
                                          (initial-contents nil initial-contents-p)
                                          fill-pointer displaced-to
                                          (displaced-index-offset 0))
  (let* ((dimensions (%dimension-list dimensions))
         (size (%product dimensions))
         (old-dimensions (array-dimensions array)))
    (if (not (eql (length dimensions) (length old-dimensions)))
        (error "adjust-array cannot change an array's rank"))
    (if (and fill-pointer (not (array-has-fill-pointer-p array)))
        (error "adjust-array cannot give a fill pointer to an array without one"))
    (let ((data (cond (displaced-to displaced-to)
                      (initial-contents-p
                       (%fill-contents (%make-simple (%element-kind element-type) size nil nil)
                                       dimensions initial-contents))
                      (t (let ((simple (%make-simple (%element-kind element-type) size
                                                     initial-element initial-element-p)))
                           (dotimes (i size simple)
                             (let ((subscripts (%subscripts i dimensions)))
                               (if (every #'< subscripts old-dimensions)
                                   (setf (aref simple i)
                                         (apply #'aref array subscripts)))))))))
          (fill (cond (fill-pointer (%fill-pointer-value fill-pointer size))
                      ((array-has-fill-pointer-p array)
                       (if (> (fill-pointer array) size)
                           (error "the fill pointer would be past the new end")
                           (fill-pointer array)))
                      (t nil))))
      (if (adjustable-array-p array)
          (progn (%set-array-header-slot array 0 data)
                 (%set-array-header-slot array 1 (if displaced-to displaced-index-offset 0))
                 (%set-array-header-slot array 2 fill)
                 (%set-array-header-slot array 3 dimensions)
                 (%set-array-header-slot array 4 (if displaced-to t nil))
                 array)
          (if (and (eql (length dimensions) 1) (null fill) (not displaced-to))
              data
              (%make-array-header data (if displaced-to displaced-index-offset 0) fill
                                  dimensions (if displaced-to t nil) nil))))))

;;; ------------------------------------------------------------------------
;;; Vectors and bits

;;; `svref` reads a simple vector only.
(defun svref (vector index)
  (if (not (simple-vector-p vector)) (error 'type-error "a simple vector" vector))
  (aref vector index))

(defun bit (array &rest subscripts) (apply #'aref array subscripts))
(defun sbit (array &rest subscripts) (apply #'aref array subscripts))
(defun %set-bit (value array &rest subscripts)
  (apply #'%set-aref array (append subscripts (list value))))
(defsetf bit (array &rest subscripts) (value) `(%set-bit ,value ,array ,@subscripts))
(defsetf sbit (array &rest subscripts) (value) `(%set-bit ,value ,array ,@subscripts))

;;; The bit operations (CLHS `bit-and`): element by element on arrays of
;;; bits with the same dimensions. RESULT nil makes a new array, t stores
;;; into the first, and an array is stored into.
(defun %bit-operation (function first second result)
  (if (not (equal (array-dimensions first) (array-dimensions second)))
      (error "bit operations want arrays of the same dimensions"))
  (let ((out (cond ((null result) (make-array (array-dimensions first) :element-type 'bit))
                   ((eq result t) first)
                   (t result))))
    (dotimes (i (array-total-size first) out)
      (setf (row-major-aref out i)
            (funcall function (row-major-aref first i) (row-major-aref second i))))))

(defun bit-and (a b &optional result) (%bit-operation (lambda (x y) (* x y)) a b result))
(defun bit-ior (a b &optional result) (%bit-operation (lambda (x y) (max x y)) a b result))
(defun bit-xor (a b &optional result) (%bit-operation (lambda (x y) (if (eql x y) 0 1)) a b result))
(defun bit-eqv (a b &optional result) (%bit-operation (lambda (x y) (if (eql x y) 1 0)) a b result))
(defun bit-nand (a b &optional result) (%bit-operation (lambda (x y) (- 1 (* x y))) a b result))
(defun bit-nor (a b &optional result) (%bit-operation (lambda (x y) (- 1 (max x y))) a b result))
(defun bit-andc1 (a b &optional result) (%bit-operation (lambda (x y) (* (- 1 x) y)) a b result))
(defun bit-andc2 (a b &optional result) (%bit-operation (lambda (x y) (* x (- 1 y))) a b result))
(defun bit-orc1 (a b &optional result) (%bit-operation (lambda (x y) (max (- 1 x) y)) a b result))
(defun bit-orc2 (a b &optional result) (%bit-operation (lambda (x y) (max x (- 1 y))) a b result))
(defun bit-not (a &optional result) (%bit-operation (lambda (x y) y (- 1 x)) a a result))

;;; A copy is a fresh simple sequence of the same kind; `subseq` knows how
;;; to make one, bit vectors included. The prelude's made every vector a
;;; general one.
(defun copy-seq (sequence) (subseq sequence 0))

;;; A string that Rust's string primitives take: a simple one.
(defun %simple-string (string)
  (if (simple-string-p string) string (copy-seq string)))

;;; `copy-structure` is in `structures.lisp` (#73).
