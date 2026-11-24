;;; Step Z (#73): `defstruct`, all of CLHS 8. Options, `:include`, BOA
;;; constructors, structures of `:type list` and `vector`, printing as
;;; `#S(...)` and reading it back, `equalp`, `copy-structure`. Tier 1:
;;; compiled and SBCL only.

;; A structure is its own type: not a vector, and printed as #S.
(defstruct place x (y 2 :read-only t))
(list (vectorp (make-place)) (arrayp (make-place)) (place-p (make-place)) (place-p #(place 1 2)))
(make-place :x '(1 2))
(prin1-to-string (make-place :x "s"))
(princ-to-string (make-place :x "s"))
(place-y (make-place :y 5))
(let ((p (make-place))) (setf (place-x p) 'set) (place-x p))

;; An accessor refuses anything but its own type and the types that include it.
(handler-case (place-x 3) (type-error () :type-error))
(defstruct other a)
(handler-case (place-x (make-other)) (type-error () :type-error))

;; :include: the parent's slots first, an initform overridden, the parent's
;; predicate and accessors accepting the child.
(defstruct (place3 (:include place (x 10))) z)
(make-place3 :z 9)
(list (place-p (make-place3)) (place3-p (make-place)) (place-x (make-place3)) (place3-x (make-place3)))
(copy-structure (make-place3 :z 1))

;; The names of things.
(defstruct (named-things (:conc-name "BLAH-") (:copier copy-it) (:predicate is-it)) a)
(list (blah-a (make-named-things :a 1)) (is-it (copy-it (make-named-things))))
(defstruct (no-prefix (:conc-name nil)) nx)
(defstruct (no-prefix2 (:include no-prefix) (:conc-name nil)) ny)
(list (nx (make-no-prefix2 :nx 3)) (nx (make-no-prefix :nx 4)))
(defstruct (char-prefix (:conc-name #\Q)) ab)
(qab (make-char-prefix :ab 'char))

;; BOA constructors (CLHS 3.4.6): an optional or keyword parameter with no
;; default takes the slot's initform, a slot the lambda list does not name
;; takes its initform too.
(defstruct (boa (:constructor boa-con (a &optional b &aux (c 7))))
  (a 1) (b 2) (c 3) (e 5))
(boa-con 9)
(boa-con 9 8)
(defstruct (boa2 (:constructor new-boa2 (a &key (b 3) c))) a b c)
(new-boa2 1 :c 4)
(fboundp 'make-boa2)
(handler-case (eval '(defstruct (boa5 (:constructor nil) (:constructor new-boa5)) a))
  (error () :error))
(defstruct (boa6 (:constructor nil)) a)
(fboundp 'make-boa6)
(defstruct (boa3 (:constructor make-boa3) (:constructor boa3-of (b a))) a b)
(list (make-boa3 :a 1 :b 2) (boa3-of 1 2))
(defstruct (boa4 (:constructor boa4-con (&rest a))) a)
(boa4-a (boa4-con 1 2 3))

;; Structures that are lists and vectors.
(defstruct (lp (:type list) :named) x)
(defstruct (lp2 (:type list) :named (:include lp)) y)
(make-lp2 :x 1 :y 2)
(list (lp-p (make-lp2)) (lp2-p (make-lp2)) (lp2-p (make-lp)) (lp2-y (make-lp2 :y 'why)))
(defstruct (vp (:type vector) :named (:initial-offset 1)) pp q)
(length (make-vp))
(list (vp-p (make-vp)) (vp-q (make-vp :q 'queue)) (aref (make-vp) 1))
(defstruct (unnamed (:type list)) a b)
(make-unnamed :a 1 :b 2)
(let ((u (make-unnamed))) (setf (unnamed-b u) 'bee) u)

;; Printing: *print-length* counts slots, #S is a level, labels for shared
;; and circular structure.
(let ((*print-length* 1)) (prin1-to-string (make-place)))
(let ((*print-level* 1)) (prin1-to-string (list (make-place :x (make-place)))))
(let ((*print-circle* t) (x (make-place))) (setf (place-x x) x) (prin1-to-string x))
(let ((*print-circle* t) (x (list 1))) (prin1-to-string (make-place3 :x x :z x)))

;; :print-function and :print-object, inherited by an including type.
(defstruct (pf (:print-function (lambda (o s d) (format s "<PF ~a ~a>" (pf-a o) d)))) a)
(prin1-to-string (make-pf :a 3))
(defstruct (pf2 (:include pf)) b)
(prin1-to-string (list (make-pf2 :a 4)))
(defstruct (po (:print-object (lambda (o s) (format s "<PO ~a>" (po-a o))))) a)
(prin1-to-string (make-po :a 5))

;; #S reads what the printer writes.
(read-from-string "#S(PLACE :X 5)")
(equalp (read-from-string (prin1-to-string (make-place3 :x '(a "b") :z 1)))
        (make-place3 :x '(a "b") :z 1))

;; equalp descends into structures of one type; equal does not.
(list (equalp (make-place :x "A") (make-place :x "a"))
      (equal (make-place) (make-place))
      (equalp (make-place) (make-place3)))
(let ((h (make-hash-table :test 'equalp)))
  (setf (gethash (make-place :x "key") h) 'found)
  (gethash (make-place :x "KEY") h))

;; Options CLHS forbids together: a predicate for an unnamed typed
;; structure, and a name in a vector that cannot hold a symbol.
(handler-case (eval '(defstruct (bad-1 (:type vector) (:predicate bad-1-p)) a)) (error () :error))
(handler-case (eval '(defstruct (bad-2 (:type (vector single-float)) :named) a)) (error () :error))
