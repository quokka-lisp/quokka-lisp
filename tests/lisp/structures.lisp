;;; defstruct.
;;;
;;; Quokka's structures are vectors with the type name in slot 0; Common
;;; Lisp's are a genuine type that `typep` knows and that prints as
;;; `#S(POINT :X 1 :Y 2)`. So **nothing here prints a structure** — the tests
;;; ask accessors and predicates, which both implementations agree about.
;;;
;;; Nor does anything here hand `point-p` a vector someone else built with
;;; the symbol `point` in slot 0. Quokka would say true and SBCL false, and
;;; that divergence is the price of having no class system until phase 3.

(defstruct point x y)

;; defstruct yields the name.
(defstruct unused-one a)

;; Constructor and accessors.
(point-x (make-point :x 1 :y 2))
(point-y (make-point :x 1 :y 2))
(point-x (make-point :y 2 :x 1))
(list (point-x (make-point :x 'a :y 'b)) (point-y (make-point :x 'a :y 'b)))

;; A slot that was not supplied is nil.
(point-x (make-point))
(point-y (make-point :x 1))

;; The predicate.
(point-p (make-point))
(point-p 5)
(point-p nil)
(point-p "text")
(point-p '(1 2))

;; Slots can have defaults.
(defstruct person name (age 0) (alive t))
(person-age (make-person :name 'ada))
(person-alive (make-person :name 'ada))
(person-name (make-person :name 'ada))
(person-age (make-person :name 'ada :age 36))
(person-alive (make-person :name 'ada :alive nil))
(person-name (make-person))

;; A default can be any form, evaluated when the constructor runs.
(defstruct counted (tag (list 'fresh)))
(counted-tag (make-counted))
(eq (counted-tag (make-counted)) (counted-tag (make-counted)))

;; Accessors are settable, which is what an extensible `setf` is for.
(let ((p (make-point :x 1 :y 2)))
  (setf (point-x p) 'moved)
  (list (point-x p) (point-y p)))
(let ((p (make-point :x 1 :y 2)))
  (setf (point-x p) 'a (point-y p) 'b)
  (list (point-x p) (point-y p)))
(let ((p (make-point))) (setf (point-x p) 'value))

;; Setting one structure does not touch another.
(let ((a (make-point :x 1)) (b (make-point :x 1)))
  (setf (point-x a) 'changed)
  (list (point-x a) (point-x b)))

;; The copier makes a separate object with the same contents.
(let ((p (make-point :x 1 :y 2)))
  (list (point-x (copy-point p)) (point-y (copy-point p))))
(let ((p (make-point :x 1)))
  (eq p (copy-point p)))
(let ((p (make-point :x 1)))
  (let ((q (copy-point p)))
    (setf (point-x q) 'only-q)
    (list (point-x p) (point-x q))))
(point-p (copy-point (make-point)))

;; Structures nest, and hold anything.
(defstruct segment from to)
(let ((s (make-segment :from (make-point :x 0 :y 0)
                       :to (make-point :x 3 :y 4))))
  (list (point-x (segment-from s)) (point-y (segment-to s))))
(let ((s (make-segment :from (list 1 2) :to "text")))
  (list (segment-from s) (segment-to s)))

;; A structure in a hash table, keyed by something hashable.
(let ((h (make-hash-table)))
  (setf (gethash 'origin h) (make-point :x 0 :y 0))
  (point-x (gethash 'origin h)))

;; A one-slot and a no-slot structure both work.
(defstruct wrapper value)
(wrapper-value (make-wrapper :value 'inside))
(defstruct marker)
(marker-p (make-marker))
