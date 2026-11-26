;;;; type-algebra.lisp — `subtypep`, by an algebra of canonical types
;;;; (phase 3, step A1, #124)
;;;;
;;;; Is every integer from 1 to 5 a real above 0 and at most 6? Is
;;;; `(and integer (not fixnum))` the same as `bignum`? `typep` cannot
;;;; answer that: it looks at one object, and these questions are about
;;;; every object there could ever be. `subtypep` answers them by turning
;;;; each type specifier into a **canonical type** (a *ctype*, the name SBCL
;;;; gives the same idea), in which two specifiers that name the same set
;;;; are written the same way, and then asking one question: is A and not B
;;;; empty? If nothing is in A without also being in B, A is a subtype of B.
;;;;
;;;; A ctype divides the world into **kinds** of object that do not overlap:
;;;; integers, ratios, floats, characters, keywords, other symbols, conses,
;;;; arrays, structures, conditions, functions, hash tables, and a few
;;;; more. For each kind it says which objects of that kind are in the type,
;;;; in a form fitted to the kind: integers as a list of intervals,
;;;; characters as intervals of character codes, symbols as a set (or "every
;;;; symbol but these"), conses as pairs of a car type and a cdr type, and
;;;; so on. Because the kinds are disjoint, `and`, `or` and `not` work kind
;;;; by kind, and the type is empty when every kind's part is.
;;;;
;;;; This is the method of SBCL's `ctype` system, and the classic way to
;;;; decide subtyping for a type language with `and`, `or` and `not`: put
;;;; the types in a normal form that is a union over disjoint pieces, so
;;;; that emptiness can be read off. Henry Baker's "A Decision Procedure for
;;;; Common Lisp's SUBTYPEP Predicate" (Lisp and Symbolic Computation, 1992)
;;;; describes the same approach.
;;;;
;;;; Two things the algebra cannot see into: `(satisfies f)`, an arbitrary
;;;; predicate, and a `(function ...)` type. Each is treated as an unknown
;;;; set (see `subtypep` at the end); where the answer depends on what that
;;;; set holds, `subtypep` says "don't know", as CLHS lets it.

;;; ------------------------------------------------------------------------
;;; Intervals
;;;
;;; An interval is a list (LOW LOW-OPEN HIGH HIGH-OPEN). A bound of nil is
;;; unbounded; otherwise it is a rational, and LOW-OPEN or HIGH-OPEN says the
;;; bound itself is excluded. An integer interval is kept closed, with
;;; integer bounds. A set of reals is a list of disjoint intervals, sorted.

(defun %make-interval (low low-open high high-open) (list low low-open high high-open))
(defun %iv-low (iv) (first iv))
(defun %iv-low-open (iv) (second iv))
(defun %iv-high (iv) (third iv))
(defun %iv-high-open (iv) (fourth iv))

;;; An integer interval with integer bounds and no open ends, or nil when
;;; no integer is in it.
(defun %integer-interval (iv)
  (let ((low (%iv-low iv)) (high (%iv-high iv)))
    (let ((low (cond ((null low) nil)
                     ((%iv-low-open iv) (+ (floor low) 1))
                     (t (ceiling low))))
          (high (cond ((null high) nil)
                      ((%iv-high-open iv) (- (ceiling high) 1))
                      (t (floor high)))))
      (if (and low high (> low high)) nil (%make-interval low nil high nil)))))

(defun %interval-empty-p (iv)
  (let ((low (%iv-low iv)) (high (%iv-high iv)))
    (and low high
         (or (> low high)
             (and (= low high) (or (%iv-low-open iv) (%iv-high-open iv)))))))

;;; Does lower bound A start before lower bound B? Unbounded first; at equal
;;; values, a closed bound before an open one.
(defun %low-before-p (a b)
  (cond ((null (%iv-low b)) nil)
        ((null (%iv-low a)) t)
        ((< (%iv-low a) (%iv-low b)) t)
        ((> (%iv-low a) (%iv-low b)) nil)
        (t (and (not (%iv-low-open a)) (%iv-low-open b)))))

;;; The intersection of two intervals, or nil.
(defun %interval-and (a b integer)
  (let* ((low-from (if (%low-before-p a b) b a))
         (high-from (cond ((null (%iv-high a)) b)
                          ((null (%iv-high b)) a)
                          ((< (%iv-high a) (%iv-high b)) a)
                          ((> (%iv-high a) (%iv-high b)) b)
                          ((%iv-high-open a) a)
                          (t b)))
         (iv (%make-interval (%iv-low low-from) (%iv-low-open low-from)
                             (%iv-high high-from) (%iv-high-open high-from))))
    (cond (integer (%integer-interval iv))
          ((%interval-empty-p iv) nil)
          (t iv))))

;;; Do A and B, A starting no later, overlap or touch, so that their union
;;; is one interval? Integer intervals touch when one ends just before the
;;; other starts.
(defun %intervals-join-p (a b integer)
  (let ((high (%iv-high a)) (low (%iv-low b)))
    (cond ((or (null high) (null low)) t)
          (integer (<= low (+ high 1)))
          ((< low high) t)
          ((= low high) (not (and (%iv-high-open a) (%iv-low-open b))))
          (t nil))))

(defun %interval-join (a b)
  (let ((high-from (cond ((null (%iv-high a)) a)
                         ((null (%iv-high b)) b)
                         ((> (%iv-high a) (%iv-high b)) a)
                         ((< (%iv-high a) (%iv-high b)) b)
                         ((%iv-high-open a) b)
                         (t a))))
    (%make-interval (%iv-low a) (%iv-low-open a) (%iv-high high-from) (%iv-high-open high-from))))

;;; The union of two interval sets, sorted and with joined intervals merged.
(defun %intervals-or (a b integer)
  (let ((sorted (sort (append a b) #'%low-before-p))
        (result nil))
    (dolist (iv sorted (nreverse result))
      (if (and result (%intervals-join-p (car result) iv integer))
          (setf (car result) (%interval-join (car result) iv))
          (push iv result)))))

(defun %intervals-and (a b integer)
  (let ((result nil))
    (dolist (x a)
      (dolist (y b)
        (let ((iv (%interval-and x y integer)))
          (when iv (push iv result)))))
    (%intervals-or result nil integer)))

;;; The complement of an interval set within UNIVERSE, itself an interval
;;; set: the gaps between the intervals, intersected with the universe.
(defun %intervals-not (a integer universe)
  (let ((gaps nil) (low nil) (low-open nil))
    (dolist (iv a)
      (when (%iv-low iv)
        (push (%make-interval low low-open (%iv-low iv) (not (%iv-low-open iv))) gaps))
      (setq low (%iv-high iv) low-open (not (%iv-high-open iv)))
      (unless low (setq gaps (cons :done gaps))))
    (unless (member :done gaps)
      (push (%make-interval low low-open nil nil) gaps))
    (%intervals-and (remove :done gaps) universe integer)))

(defun %intervals-contain-p (intervals x)
  (dolist (iv intervals nil)
    (when (and (or (null (%iv-low iv))
                   (if (%iv-low-open iv) (> x (%iv-low iv)) (>= x (%iv-low iv))))
               (or (null (%iv-high iv))
                   (if (%iv-high-open iv) (< x (%iv-high iv)) (<= x (%iv-high iv)))))
      (return t))))

(defun %whole-line () (list (%make-interval nil nil nil nil)))
(defun %character-codes () (list (%make-interval 0 nil (- char-code-limit 1) nil)))

;;; ------------------------------------------------------------------------
;;; Kinds
;;;
;;; Every object is of exactly one kind. Each kind's part of a ctype is a
;;; *restriction*, nil when the type holds no object of that kind. Its form
;;; depends on the kind's *shape*:
;;;
;;;   :integers  integer intervals        :reals    intervals of rationals
;;;   :codes     character-code intervals :symbols  (:in . list) or (:not-in . list)
;;;   :pairs     list of (car-ctype . cdr-ctype)
;;;   :arrays    list of (element-type simple pattern exceptions)
;;;   :tree      list of (roots . exceptions), names in a hierarchy
;;;   :parts     the complex numbers' part types, a subset of (:rational :float)
;;;   :whole     t: every object of the kind

(defvar %kinds
  '((:integer . :integers) (:ratio . :reals) (:float . :reals) (:negative-zero . :whole)
    (:complex . :parts)
    (:character . :codes) (:symbol . :symbols) (:keyword . :symbols) (:cons . :pairs)
    (:array . :arrays) (:structure . :tree) (:condition . :tree) (:instance . :tree)
    (:compiled-function . :whole) (:function . :whole) (:hash-table . :whole)
    (:random-state . :whole) (:pathname . :whole) (:logical-pathname . :whole)
    (:other . :whole)))

(defun %shape (kind) (cdr (assoc kind %kinds)))

;;; What kind OBJECT is.
(defun %object-kind (object)
  (cond ((integerp object) :integer)
        ((%ratiop object) :ratio)
        ((floatp object) (if (%negative-zero-p object) :negative-zero :float))
        ((characterp object) :character)
        ((keywordp object) :keyword)
        ((symbolp object) :symbol)
        ((consp object) :cons)
        ((arrayp object) :array)
        ((%instance-wrapper object) :instance)
        ((%conditionp object) :condition)
        ((%structure-layout object) :structure)
        ((functionp object) (if (compiled-function-p object) :compiled-function :function))
        ((hash-table-p object) :hash-table)
        (t :other)))

;;; -0.0 is its own kind. It equals 0.0, so a range of floats that holds 0.0
;;; holds it too, but it is not `eql` to 0.0, so `(member 0.0)` does not:
;;; `(float 0.0 1.0)` is not a subtype of `(or (float (0.0) 1.0) (member
;;; 0.0))`. An interval cannot tell the two zeros apart; a kind can.
(defun %negative-zero-p (x) (and (zerop x) (minusp (float-sign x))))

;;; ------------------------------------------------------------------------
;;; Ctypes
;;;
;;; A ctype holds a restriction per kind (PARTS, an alist with only the
;;; kinds that are not empty) and two lists of particular objects. The
;;; objects are those `member` and `eql` name that no restriction can
;;; express — a particular cons, string or structure — and the type is
;;; (PARTS without OUT) together with IN. Numbers, characters and symbols
;;; never go there: a restriction can name each of them exactly.
;;;
;;; IN holds only objects PARTS does not, and OUT only objects PARTS does,
;;; and every operation keeps it so. Because a non-empty restriction of a
;;; kind that can only go into IN or OUT holds infinitely many objects
;;; (there is no end of fresh conses), taking a few out never empties it.

(defstruct (%ctype (:constructor %make-ctype (parts in out))) parts in out)

(defun %empty-ctype () (%make-ctype nil nil nil))

(defun %universe ()
  (%make-ctype (mapcar (lambda (entry) (cons (car entry) (%full (car entry)))) %kinds) nil nil))

(defun %kind-ctype (kind restriction)
  (if restriction (%make-ctype (list (cons kind restriction)) nil nil) (%empty-ctype)))

(defun %kinds-ctype (kinds)
  (%make-ctype (mapcar (lambda (kind) (cons kind (%full kind))) kinds) nil nil))

(defun %part (ctype kind) (cdr (assoc kind (%ctype-parts ctype))))

;;; Does the ctype's restriction of OBJECT's kind hold OBJECT?
(defun %parts-contain-p (parts object)
  (let* ((kind (%object-kind object))
         (restriction (cdr (assoc kind parts))))
    (and restriction (%restriction-contains-p kind restriction object))))

(defun %ctype-contains-p (ctype object)
  (cond ((member object (%ctype-in ctype)) t)
        ((member object (%ctype-out ctype)) nil)
        (t (%parts-contain-p (%ctype-parts ctype) object))))

(defun %ctype-and (a b)
  (let ((parts nil))
    (dolist (entry (%ctype-parts a))
      (let ((other (assoc (car entry) (%ctype-parts b))))
        (when other
          (let ((restriction (%restriction-and (car entry) (cdr entry) (cdr other))))
            (when restriction (push (cons (car entry) restriction) parts))))))
    (%make-ctype
     parts
     (append (remove-if-not (lambda (x) (%ctype-contains-p b x)) (%ctype-in a))
             (remove-if-not (lambda (x) (%ctype-contains-p a x)) (%ctype-in b)))
     (remove-if-not (lambda (x) (%parts-contain-p parts x))
                    (union (%ctype-out a) (%ctype-out b))))))

(defun %ctype-or (a b)
  (let ((parts nil))
    (dolist (entry %kinds)
      (let* ((kind (car entry))
             (x (assoc kind (%ctype-parts a)))
             (y (assoc kind (%ctype-parts b)))
             (restriction (cond ((and x y) (%restriction-or kind (cdr x) (cdr y)))
                                (x (cdr x))
                                (y (cdr y))
                                (t nil))))
        (when restriction (push (cons kind restriction) parts))))
    (%make-ctype
     parts
     (remove-if (lambda (x) (%parts-contain-p parts x))
                (union (%ctype-in a) (%ctype-in b)))
     (remove-if (lambda (x) (or (%ctype-contains-p a x) (%ctype-contains-p b x)))
                (union (%ctype-out a) (%ctype-out b))))))

(defun %ctype-not (a)
  (let ((parts nil))
    (dolist (entry %kinds)
      (let* ((kind (car entry))
             (x (assoc kind (%ctype-parts a)))
             (restriction (if x (%restriction-not kind (cdr x)) (%full kind))))
        (when restriction (push (cons kind restriction) parts))))
    (%make-ctype parts (%ctype-out a) (%ctype-in a))))

;;; Is the ctype empty? Two values, as `subtypep` returns them: empty, and
;;; whether that answer is certain.
(defun %ctype-empty-p (ctype)
  (if (%ctype-in ctype)
      (values nil t)
      (let ((certain t))
        (dolist (entry (%ctype-parts ctype) (values certain certain))
          (multiple-value-bind (empty sure) (%restriction-empty-p (car entry) (cdr entry))
            (cond ((and (not empty) sure) (return (values nil t)))
                  ((not sure) (setq certain nil))))))))

;;; ------------------------------------------------------------------------
;;; Restrictions, by shape

(defun %full (kind)
  (case (%shape kind)
    (:integers (%whole-line))
    (:reals (%whole-line))
    (:codes (%character-codes))
    (:symbols (list :not-in))
    (:pairs (list (cons :any :any)))
    (:arrays (%all-arrays))
    (:tree (list (list nil)))
    (:parts (list :rational :float))
    (t t)))

(defun %restriction-and (kind a b)
  (case (%shape kind)
    (:integers (%intervals-and a b t))
    ((:reals :codes) (%intervals-and a b (eq (%shape kind) :codes)))
    (:symbols (%symbols-and a b))
    (:pairs (%pairs-and a b))
    (:arrays (%arrays-and a b))
    (:tree (%tree-and kind a b))
    (:parts (intersection a b))
    (t (and a b))))

(defun %restriction-or (kind a b)
  (case (%shape kind)
    ((:integers :codes) (%intervals-or a b t))
    (:reals (%intervals-or a b nil))
    (:symbols (%symbols-not (%symbols-and (%symbols-not a) (%symbols-not b))))
    ((:pairs :arrays :tree) (append a b))
    (:parts (union a b))
    (t (or a b))))

(defun %restriction-not (kind a)
  (case (%shape kind)
    (:integers (%intervals-not a t (%whole-line)))
    (:reals (%intervals-not a nil (%whole-line)))
    (:codes (%intervals-not a t (%character-codes)))
    (:symbols (%symbols-not a))
    (:pairs (%pairs-not a))
    (:arrays (%arrays-not a))
    (:tree (%tree-not kind a))
    (:parts (set-difference (list :rational :float) a))
    (t (not a))))

;;; A ratio is never an integer, so a ratio interval that is a single
;;; integer, as `(rational 1 1)` leaves, holds nothing. Any longer interval
;;; holds ratios.
(defun %restriction-empty-p (kind a)
  (case (%shape kind)
    ((:integers :codes) (values (null a) t))
    (:reals (values (if (eq kind :ratio)
                        (every (lambda (iv) (and (%iv-low iv) (eql (%iv-low iv) (%iv-high iv))
                                                 (integerp (%iv-low iv))))
                               a)
                        (null a))
                    t))
    (:symbols (values (and (eq (car a) :in) (null (cdr a))) t))
    (:pairs (%pairs-empty-p a))
    (:arrays (values (every #'%array-term-empty-p a) t))
    (:tree (%tree-empty-p kind a))
    (:parts (values (null a) t))
    (t (values (not a) t))))

(defun %restriction-contains-p (kind restriction object)
  (case (%shape kind)
    ((:integers :codes)
     (%intervals-contain-p restriction (if (characterp object) (char-code object) object)))
    (:reals (%intervals-contain-p restriction (rational object)))
    (:symbols (if (eq (car restriction) :in)
                  (if (member object (cdr restriction)) t nil)
                  (not (member object (cdr restriction)))))
    (:pairs (some (lambda (pair)
                    (and (%pc-contains-p (car pair) (car object))
                         (%pc-contains-p (cdr pair) (cdr object))))
                  restriction))
    (:arrays (%arrays-contain-p restriction object))
    (:tree (%tree-contains-p kind restriction object))
    (t restriction)))

;;; Symbols: (:in . symbols) is just those, (:not-in . symbols) every symbol
;;; of the kind but those. The empty set is nil.
(defun %symbols-make (tag symbols)
  (if (and (eq tag :in) (null symbols)) nil (cons tag symbols)))

(defun %symbols-and (a b)
  (let ((x (cdr a)) (y (cdr b)))
    (cond ((or (null a) (null b)) nil)
          ((and (eq (car a) :in) (eq (car b) :in)) (%symbols-make :in (intersection x y)))
          ((eq (car a) :in) (%symbols-make :in (set-difference x y)))
          ((eq (car b) :in) (%symbols-make :in (set-difference y x)))
          (t (%symbols-make :not-in (union x y))))))

(defun %symbols-not (a)
  (cond ((null a) (list :not-in))
        ((eq (car a) :in) (%symbols-make :not-in (cdr a)))
        (t (%symbols-make :in (cdr a)))))

;;; Conses: a union of pairs (CAR . CDR), each the set of conses whose car is
;;; of type CAR and cdr of type CDR. The cons types are products, so
;;; intersecting two pairs intersects their parts, and the complement of a
;;; pair (A . D) among conses is (not-A . t) or (t . not-D).
;;;
;;; A car or cdr of :any stands for the type of every object. It cannot be
;;; the universe ctype itself, because the universe holds every cons, which
;;; is the pair (:any . :any): the universe would contain itself, and
;;; building it would never end.
(defun %pc-and (a b)
  (cond ((eq a :any) b) ((eq b :any) a) (t (%ctype-and a b))))

(defun %pc-not (a) (if (eq a :any) (%empty-ctype) (%ctype-not a)))

(defun %pc-empty-p (a) (if (eq a :any) (values nil t) (%ctype-empty-p a)))

(defun %pc-contains-p (a object) (or (eq a :any) (%ctype-contains-p a object)))

(defun %pairs-and (a b)
  (let ((result nil))
    (dolist (x a (nreverse result))
      (dolist (y b)
        (let ((car-type (%pc-and (car x) (car y)))
              (cdr-type (%pc-and (cdr x) (cdr y))))
          (unless (or (%surely-empty-p car-type) (%surely-empty-p cdr-type))
            (push (cons car-type cdr-type) result)))))))

(defun %surely-empty-p (ctype)
  (multiple-value-bind (empty certain) (%pc-empty-p ctype) (and empty certain)))

(defun %pairs-not (a)
  (let ((result (%full :cons)))
    (dolist (pair a result)
      (setq result
            (%pairs-and result
                        (list (cons (%pc-not (car pair)) :any)
                              (cons :any (%pc-not (cdr pair)))))))))

(defun %pairs-empty-p (a)
  (let ((certain t))
    (dolist (pair a (values certain certain))
      (multiple-value-bind (car-empty car-sure) (%pc-empty-p (car pair))
        (multiple-value-bind (cdr-empty cdr-sure) (%pc-empty-p (cdr pair))
          (cond ((or (and car-empty car-sure) (and cdr-empty cdr-sure)))
                ((and car-sure cdr-sure) (return (values nil t)))
                (t (setq certain nil))))))))

;;; Arrays: a union of terms (ELEMENT-TYPE SIMPLE PATTERN EXCEPTIONS).
;;; ELEMENT-TYPE is an upgraded element type (t, character, bit or nil),
;;; SIMPLE is t or nil, and the term is the arrays of that element type and
;;; simplicity whose dimensions fit PATTERN and fit none of EXCEPTIONS. A
;;; pattern is :any (every rank) or a list of sizes and `*`s.
;;;
;;; Whether a pattern lies within a union of others has a simple answer:
;;; only if it lies within one of them. A pattern with a `*` holds sizes no
;;; finite set of patterns names one by one, so some single pattern must
;;; have a `*` there too.

(defvar %array-element-types '(t character bit nil))

(defun %all-arrays ()
  (let ((result nil))
    (dolist (element-type %array-element-types result)
      (dolist (simple '(t nil))
        (push (list element-type simple :any nil) result)))))

(defun %pattern-and (p q)
  (cond ((eq p :any) q)
        ((eq q :any) p)
        ((/= (length p) (length q)) :none)
        (t (let ((result nil))
             (do ((x p (cdr x)) (y q (cdr y)))
                 ((null x) (nreverse result))
               (cond ((eq (car x) '*) (push (car y) result))
                     ((or (eq (car y) '*) (eql (car x) (car y))) (push (car x) result))
                     (t (return :none))))))))

(defun %pattern-within-p (p q)
  (cond ((eq q :any) t)
        ((eq p :any) nil)
        ((/= (length p) (length q)) nil)
        (t (every (lambda (x y) (or (eq y '*) (eql x y))) p q))))

(defun %pattern-fits-p (pattern dimensions)
  (or (eq pattern :any)
      (and (= (length pattern) (length dimensions))
           (every (lambda (want have) (or (eq want '*) (eql want have))) pattern dimensions))))

(defun %array-term-empty-p (term)
  (let ((pattern (third term)))
    (or (eq pattern :none)
        (some (lambda (exception) (%pattern-within-p pattern exception)) (fourth term)))))

(defun %arrays-and (a b)
  (let ((result nil))
    (dolist (x a (nreverse result))
      (dolist (y b)
        (when (and (eq (first x) (first y)) (eq (second x) (second y)))
          (let ((pattern (%pattern-and (third x) (third y))))
            (unless (eq pattern :none)
              (push (list (first x) (second x) pattern
                          (remove-if (lambda (e) (eq (%pattern-and pattern e) :none))
                                     (append (fourth x) (fourth y))))
                    result))))))))

;;; The complement among arrays, one element type and simplicity at a time:
;;; the complement of a term (E S P X) among arrays of E and S is (E S :any
;;; (P)) together with (E S x ()) for each exception x.
(defun %arrays-not (a)
  (let ((result nil))
    (dolist (element-type %array-element-types result)
      (dolist (simple '(t nil))
        (let ((part (list (list element-type simple :any nil))))
          (dolist (term a)
            (when (and (eq (first term) element-type) (eq (second term) simple))
              (setq part
                    (%arrays-and part
                                 (cons (list element-type simple :any (list (third term)))
                                       (mapcar (lambda (x) (list element-type simple x nil))
                                               (fourth term)))))))
          (setq result (append result part)))))))

(defun %arrays-contain-p (terms array)
  (let ((element-type (array-element-type array))
        (simple (%simple-array-p array))
        (dimensions (array-dimensions array)))
    (some (lambda (term)
            (and (eq (first term) element-type)
                 (eq (second term) simple)
                 (%pattern-fits-p (third term) dimensions)
                 (notany (lambda (x) (%pattern-fits-p x dimensions)) (fourth term))))
          terms)))

;;; Structures and conditions: a union of terms (ROOTS . EXCEPTIONS), the
;;; objects of every type named in ROOTS (nil: every structure, or every
;;; condition) that are of no type named in EXCEPTIONS. Every type here can
;;; have instances of its own, not of any subtype, so a term is empty only
;;; when one of its roots lies within one of its exceptions.
;;;
;;; A structure has one parent, so two roots neither of which includes the
;;; other share no object. A condition type can have several parents, and
;;; programs will define more (phase 3, step B), so such a pair is a type
;;; that may or may not have members: "don't know". The same holds for
;;; classes, whose terms name class objects rather than names: a class may
;;; have no name, or a name that now names another class.

(defun %tree-subtype-p (kind a b)
  (case kind
    (:structure (do ((layout (%find-layout a) (%layout-include layout)))
                    ((null layout) nil)
                  (when (eq (%layout-name layout) b) (return t))))
    (:instance (if (member b (%class-precedence-list a)) t nil))
    (t (%condition-subtype-p a b))))

;;; ROOTS with every root that another root lies within dropped, or :none
;;; when two structure roots are unrelated.
(defun %tree-roots (kind roots)
  (let ((kept nil))
    (dolist (root roots kept)
      (cond ((some (lambda (other) (%tree-subtype-p kind other root)) kept))
            (t (setq kept (cons root (remove-if (lambda (other) (%tree-subtype-p kind root other))
                                                kept))))))
    (if (and (eq kind :structure) (cdr kept)) :none kept)))

;;; Terms that are surely empty are dropped as they are made, and a term
;;; already present is not added twice. Without that, a long `and` of `or`s
;;; of classes multiplies its terms at every step, and its complement
;;; multiplies them again (ansi-test tac-3.17 ran out of steps).
(defun %tree-and (kind a b)
  (let ((result nil))
    (dolist (x a (nreverse result))
      (dolist (y b)
        (let ((roots (%tree-roots kind (append (car x) (car y)))))
          (unless (eq roots :none)
            (let ((term (cons roots (union (cdr x) (cdr y)))))
              (unless (or (multiple-value-bind (empty certain) (%tree-term-empty-p kind term)
                            (and empty certain))
                          (member term result :test #'equal))
                (push term result)))))))))

(defun %tree-not (kind a)
  (let ((result (%full kind)))
    (dolist (term a result)
      (setq result
            (%tree-and kind result
                       (append (mapcar (lambda (root) (cons nil (list root))) (car term))
                               (mapcar (lambda (exception) (list (list exception))) (cdr term))))))))

(defun %tree-term-empty-p (kind term)
  (let ((roots (car term)) (exceptions (cdr term)))
    (cond ((some (lambda (root)
                   (some (lambda (exception) (%tree-subtype-p kind root exception)) exceptions))
                 roots)
           (values t t))
          ((cdr roots) (values nil nil))
          (t (values nil t)))))

(defun %tree-empty-p (kind a)
  (let ((certain t))
    (dolist (term a (values certain certain))
      (multiple-value-bind (empty sure) (%tree-term-empty-p kind term)
        (cond ((and (not empty) sure) (return (values nil t)))
              ((not sure) (setq certain nil)))))))

(defun %tree-contains-p (kind terms object)
  (let ((name (case kind
                (:structure (%layout-name (%structure-layout object)))
                (:instance (class-of object))
                (t (%condition-type object)))))
    (some (lambda (term)
            (and (every (lambda (root) (%tree-subtype-p kind name root)) (car term))
                 (notany (lambda (exception) (%tree-subtype-p kind name exception)) (cdr term))))
          terms)))

;;; ------------------------------------------------------------------------
;;; Parsing a type specifier into a ctype

;;; While `subtypep` decides one question, the `satisfies` and function
;;; types met so far, each with the set it is being supposed to be: t for
;;; every object, nil for none. See `subtypep`.
(defvar %unknown-sets nil)
(defvar %unknown-assignment nil)

(defun %unknown-set (specifier)
  (let ((entry (assoc specifier %unknown-assignment :test #'equal)))
    (unless (member specifier %unknown-sets :test #'equal)
      (push specifier %unknown-sets))
    (if (and entry (cdr entry)) (%universe) (%empty-ctype))))

;;; A real range of one kind, from a compound specifier's bounds.
(defun %bound (bound)
  (cond ((eq bound '*) (values nil nil))
        ((consp bound) (values (rational (car bound)) t))
        (t (values (rational bound) nil))))

(defun %range (kind low high)
  (multiple-value-bind (low low-open) (%bound low)
    (multiple-value-bind (high high-open) (%bound high)
      (let* ((iv (%make-interval low low-open high high-open))
             (ctype (%kind-ctype kind
                                 (if (eq kind :integer)
                                     (let ((i (%integer-interval iv))) (if i (list i) nil))
                                     (if (%interval-empty-p iv) nil (list iv))))))
        (if (and (eq kind :float) (%intervals-contain-p (list iv) 0))
            (%ctype-or ctype (%kinds-ctype '(:negative-zero)))
            ctype)))))

(defun %parse-range (kinds specifier)
  (let* ((arguments (%type-arguments specifier 2))
         (low (first arguments)) (high (second arguments))
         (result (%empty-ctype)))
    (%check-bound low specifier)
    (%check-bound high specifier)
    (dolist (kind kinds result)
      (setq result (%ctype-or result (%range kind low high))))))

(defun %integer-range (low high)
  (%range :integer (if low low '*) (if high high '*)))

(defun %code-range (low high)
  (%kind-ctype :character (list (%make-interval low nil high nil))))

;;; The ctype holding just OBJECT.
(defun %object-ctype (object)
  (let ((kind (%object-kind object)))
    (case kind
      ((:integer) (%integer-range object object))
      ((:ratio :float) (%kind-ctype kind (list (%make-interval (rational object) nil
                                                                 (rational object) nil))))
      ((:negative-zero) (%kinds-ctype '(:negative-zero)))
      ((:character) (%code-range (char-code object) (char-code object)))
      ((:symbol :keyword) (%kind-ctype kind (list :in object)))
      (t (%make-ctype nil (list object) nil)))))

;;; The ctype holding just OBJECTS, built in one pass: a `member` type of a
;;; thousand symbols, joined one object at a time, took a thousand unions
;;; each as long as the set so far.
(defun %member-ctype (objects)
  (let ((symbols nil) (keywords nil) (others nil) (points nil) (negative-zero nil))
    (dolist (object objects)
      (case (%object-kind object)
        ((:symbol) (push object symbols))
        ((:keyword) (push object keywords))
        ((:integer :ratio :float :character) (push object points))
        ((:negative-zero) (setq negative-zero t))
        (t (pushnew object others))))
    (let ((parts nil))
      (when symbols
        (push (cons :symbol (cons :in (remove-duplicates symbols))) parts))
      (when keywords
        (push (cons :keyword (cons :in (remove-duplicates keywords))) parts))
      (when negative-zero (push (cons :negative-zero t) parts))
      (let ((result (%make-ctype parts others nil)))
        (dolist (kind '(:integer :ratio :float :character) result)
          (let ((intervals nil))
            (dolist (object points)
              (when (eq (%object-kind object) kind)
                (let ((x (if (characterp object) (char-code object) (rational object))))
                  (push (%make-interval x nil x nil) intervals))))
            (when intervals
              (setq result
                    (%ctype-or result
                               (%kind-ctype kind (%intervals-or intervals nil
                                                                (member kind '(:integer :character)))))))))))))

;;; The upgraded element type of SPECIFIER, by the algebra: the first of
;;; nil, bit, character that holds it, or t (CLHS 15.1.2.1). The common
;;; names are answered at once, since `make-array` asks.
(defun upgraded-array-element-type (type &optional environment)
  environment
  (cond ((eq type t) t)
        ((member type '(character base-char standard-char)) 'character)
        ((eq type 'bit) 'bit)
        ((null type) nil)
        ((subtypep type nil) nil)
        ((subtypep type 'bit) 'bit)
        ((subtypep type 'character) 'character)
        (t t)))

;;; `make-array` and its relatives specialise by this, so an element type
;;; named through `deftype` or as `(member #\a #\b)` upgrades the way
;;; `upgraded-array-element-type` says.
(defun %element-kind (type)
  (let ((upgraded (upgraded-array-element-type type)))
    (cond ((null upgraded) :nil)
          ((eq upgraded 'character) :character)
          ((eq upgraded 'bit) :bit)
          (t :t))))

;;; Arrays of ELEMENT-TYPE (a specifier or `*`) fitting PATTERN, simple
;;; only if SIMPLE-ONLY.
(defun %array-ctype (element-type pattern simple-only)
  (let ((types (if (eq element-type '*)
                   %array-element-types
                   (list (upgraded-array-element-type element-type))))
        (terms nil))
    (dolist (type types)
      (dolist (simple (if simple-only '(t) '(t nil)))
        (push (list type simple pattern nil) terms)))
    (%kind-ctype :array terms)))

;;; A dimensions argument as a pattern: `*` any rank, an integer that rank,
;;; or a list of sizes and `*`s.
(defun %dimensions-pattern (dimensions specifier)
  (cond ((eq dimensions '*) :any)
        ((and (integerp dimensions) (>= dimensions 0)) (make-list dimensions :initial-element '*))
        ((and (listp dimensions)
              (every (lambda (d) (or (eq d '*) (and (integerp d) (>= d 0)))) dimensions))
         dimensions)
        (t (error "malformed type specifier" specifier))))

(defun %size-pattern (size specifier)
  (cond ((eq size '*) (list '*))
        ((and (integerp size) (>= size 0)) (list size))
        (t (error "malformed type specifier" specifier))))

(defun %structure-names-ctype (names)
  (%kind-ctype :structure (mapcar (lambda (name) (list (list name))) names)))

(defun %list-ctype () (%ctype-or (%kinds-ctype '(:cons)) (%kind-ctype :symbol (list :in nil))))

(defun %vectors-ctype () (%array-ctype '* (list '*) nil))

(defun %standard-char-ctype ()
  (%ctype-or (%code-range 10 10) (%code-range 32 126)))

(defun %parse-atom (name)
  (case name
    ((t) (%universe))
    ((nil) (%empty-ctype))
    ((atom) (%ctype-not (%kinds-ctype '(:cons))))
    ((list) (%list-ctype))
    ((cons) (%kinds-ctype '(:cons)))
    ((null) (%kind-ctype :symbol (list :in nil)))
    ((symbol) (%kinds-ctype '(:symbol :keyword)))
    ((keyword) (%kinds-ctype '(:keyword)))
    ((boolean) (%kind-ctype :symbol (list :in t nil)))
    ((number) (%kinds-ctype '(:integer :ratio :float :negative-zero :complex)))
    ((real) (%kinds-ctype '(:integer :ratio :float :negative-zero)))
    ((rational) (%kinds-ctype '(:integer :ratio)))
    ((integer signed-byte) (%kinds-ctype '(:integer)))
    ((ratio) (%kinds-ctype '(:ratio)))
    ((float short-float single-float double-float long-float)
     (%kinds-ctype '(:float :negative-zero)))
    ((fixnum) (%integer-range most-negative-fixnum most-positive-fixnum))
    ((bignum) (%ctype-and (%kinds-ctype '(:integer))
                          (%ctype-not (%integer-range most-negative-fixnum most-positive-fixnum))))
    ((bit) (%integer-range 0 1))
    ((unsigned-byte) (%integer-range 0 nil))
    ((complex) (%kinds-ctype '(:complex)))
    ((character) (%kinds-ctype '(:character)))
    ((base-char) (%code-range 0 127))
    ((standard-char) (%standard-char-ctype))
    ((extended-char) (%code-range 128 (- char-code-limit 1)))
    ((array) (%array-ctype '* :any nil))
    ((simple-array) (%array-ctype '* :any t))
    ((vector) (%vectors-ctype))
    ((simple-vector) (%array-ctype t (list '*) t))
    ((string base-string) (%array-ctype 'character (list '*) nil))
    ((simple-string simple-base-string) (%array-ctype 'character (list '*) t))
    ((bit-vector) (%array-ctype 'bit (list '*) nil))
    ((simple-bit-vector) (%array-ctype 'bit (list '*) t))
    ((sequence) (%ctype-or (%list-ctype) (%vectors-ctype)))
    ;; A funcallable instance, such as a generic function, is a function:
    ;; its class has `function` among its superclasses.
    ((function) (%ctype-or (%kinds-ctype '(:compiled-function :function))
                           (%kind-ctype :instance (list (list (list (find-class 'function)))))))
    ((standard-object) (%class-ctype (find-class 'standard-object)))
    ((compiled-function) (%kinds-ctype '(:compiled-function)))
    ((hash-table) (%kinds-ctype '(:hash-table)))
    ((random-state) (%kinds-ctype '(:random-state)))
    ((pathname) (%kinds-ctype '(:pathname :logical-pathname)))
    ((logical-pathname) (%kinds-ctype '(:logical-pathname)))
    ((package) (%structure-names-ctype '(%package)))
    ((readtable) (%structure-names-ctype '(%readtable)))
    ((stream) (%structure-names-ctype '(%string-input-stream %string-output-stream %console-stream)))
    ((string-stream) (%structure-names-ctype '(%string-input-stream %string-output-stream)))
    ((structure-object) (%kinds-ctype '(:structure)))
    ((condition) (%kinds-ctype '(:condition)))
    (t (cond ((%structure-type-layout name) (%structure-names-ctype (list name)))
             ((%condition-entry name %condition-supertypes)
              (%kind-ctype :condition (list (list (list name)))))
             ((find-class name nil) (%class-ctype (find-class name nil)))
             (t (throw '%unknown-type nil))))))

;;; The type a class is. A class `defclass` made is a term of the `:instance`
;;; kind, rooted at the class; any other class is the type its name names,
;;; since every built-in, structure and condition class's name is one.
(defun %class-ctype (class)
  (if (member (class-of class) (list (find-class 'standard-class)
                                     (find-class 'funcallable-standard-class)
                                     (find-class 'forward-referenced-class)))
      (%kind-ctype :instance (list (list (list class))))
      (%parse-type (%class-name class))))

(defun %parse-compound (head specifier)
  (case head
    ((and) (let ((result (%universe)))
             (dolist (type (cdr specifier) result)
               (setq result (%ctype-and result (%parse-type type))))))
    ((or) (let ((result (%empty-ctype)))
            (dolist (type (cdr specifier) result)
              (setq result (%ctype-or result (%parse-type type))))))
    ((not) (%ctype-not (%parse-type (first (%type-arguments specifier 1)))))
    ((member)
     (if (or (not (listp (last specifier))) (cdr (last specifier)))
         (error "malformed type specifier" specifier))
     (%member-ctype (cdr specifier)))
    ((eql) (%object-ctype (first (%type-arguments specifier 1))))
    ((satisfies)
     (unless (symbolp (first (%type-arguments specifier 1)))
       (error "malformed type specifier" specifier))
     (%unknown-set specifier))
    ((cons)
     ;; `*` here means any object, as `t` does.
     (let ((arguments (%type-arguments specifier 2)))
       (%kind-ctype :cons
                    (list (cons (if (eq (first arguments) '*) :any (%parse-type (first arguments)))
                                (if (eq (second arguments) '*) :any
                                    (%parse-type (second arguments))))))))
    ((integer) (%parse-range '(:integer) specifier))
    ((rational) (%parse-range '(:integer :ratio) specifier))
    ((real) (%parse-range '(:integer :ratio :float) specifier))
    ((float short-float single-float double-float long-float) (%parse-range '(:float) specifier))
    ((mod)
     (let ((n (first (%type-arguments specifier 1))))
       (unless (and (integerp n) (> n 0)) (error "malformed type specifier" specifier))
       (%integer-range 0 (- n 1))))
    ((signed-byte unsigned-byte)
     (let ((size (first (%type-arguments specifier 1))))
       (cond ((eq size '*) (%parse-atom head))
             ((not (and (integerp size) (> size 0))) (error "malformed type specifier" specifier))
             ((eq head 'unsigned-byte) (%integer-range 0 (- (ash 1 size) 1)))
             (t (%integer-range (- (ash 1 (- size 1))) (- (ash 1 (- size 1)) 1))))))
    ((array simple-array)
     (let ((arguments (%type-arguments specifier 2)))
       (%array-ctype (first arguments) (%dimensions-pattern (second arguments) specifier)
                     (eq head 'simple-array))))
    ((vector)
     (let ((arguments (%type-arguments specifier 2)))
       (%array-ctype (first arguments) (%size-pattern (second arguments) specifier) nil)))
    ((simple-vector string simple-string base-string simple-base-string bit-vector simple-bit-vector)
     (let ((pattern (%size-pattern (first (%type-arguments specifier 1)) specifier)))
       (%array-ctype (cond ((member head '(simple-vector)) t)
                           ((member head '(bit-vector simple-bit-vector)) 'bit)
                           (t 'character))
                     pattern
                     (member head '(simple-vector simple-string simple-base-string
                                    simple-bit-vector)))))
    ;; A complex number's parts are upgraded as SBCL upgrades them, to
    ;; rational or float (CLHS `upgraded-complex-part-type`).
    ((complex)
     (let ((part (first (%type-arguments specifier 1))))
       (%kind-ctype :complex
                    (cond ((eq part '*) (list :rational :float))
                          ((subtypep part 'rational) (list :rational))
                          ((subtypep part 'float) (list :float))
                          ((subtypep part 'real) (list :rational :float))
                          (t (error "malformed type specifier" specifier))))))
    ;; A function type says what a function accepts and returns, which no
    ;; one can check by looking: it is some set of functions, unknown.
    ((function)
     (%ctype-and (%kinds-ctype '(:compiled-function :function)) (%unknown-set specifier)))
    ((values) (error "a values type specifier is not a type" specifier))
    (t (if (or (%structure-type-layout head) (gethash head %atomic-types))
           (error "malformed type specifier" specifier)
           (throw '%unknown-type nil)))))

(defun %parse-type (specifier)
  (let ((specifier (%expand-type specifier)))
    (cond ((consp specifier) (%parse-compound (car specifier) specifier))
          ((symbolp specifier) (%parse-atom specifier))
          ((%class-object-p specifier) (%class-ctype specifier))
          (t (error "malformed type specifier" specifier)))))

;;; ------------------------------------------------------------------------
;;; subtypep

;;; Is TYPE-1 and not TYPE-2 empty, supposing each unknown set is what the
;;; assignment says?
(defun %difference-empty-p (type-1 type-2)
  (%ctype-empty-p (%ctype-and (%parse-type type-1) (%ctype-not (%parse-type type-2)))))

;;; Every way of supposing each of SETS to be everything or nothing.
(defun %assignments (sets)
  (if (null sets)
      (list nil)
      (let ((rest (%assignments (cdr sets))))
        (append (mapcar (lambda (a) (cons (cons (car sets) t) a)) rest)
                (mapcar (lambda (a) (cons (cons (car sets) nil) a)) rest)))))

;;; When an unknown set is involved: whether an object is in A and not B
;;; depends only on which of the unknown sets it is in. So A and not B is
;;; empty for every possible meaning of the sets exactly when it is empty
;;; with each set supposed to be everything or nothing, in every
;;; combination; then TYPE-1 is surely a subtype. If it is not empty under
;;; some combination, the true sets might still make it empty, so the
;;; answer is "don't know". SBCL answers the same way.
(defun %subtypep (type-1 type-2)
  (let ((%unknown-sets nil) (%unknown-assignment nil))
    (multiple-value-bind (empty certain) (%difference-empty-p type-1 type-2)
      (cond ((null %unknown-sets) (values empty certain))
            ((> (length %unknown-sets) 6) (values nil nil))
            (t (dolist (assignment (%assignments %unknown-sets) (values t t))
                 (setq %unknown-assignment assignment)
                 (multiple-value-bind (empty certain) (%difference-empty-p type-1 type-2)
                   (unless (and empty certain) (return (values nil nil))))))))))

(defun subtypep (type-1 type-2 &optional environment)
  environment
  ;; The same specifier is the same type, whatever is in it, even an
  ;; unknown one. Only the same object counts: two specifiers that are
  ;; `equal` may still differ, since `(member "abc")` and another `(member
  ;; "abc")`, or two `(member (a b c))` with different lists in them, name
  ;; different objects.
  (if (eq type-1 type-2)
      (values t t)
      (let ((answer (catch '%unknown-type (multiple-value-list (%subtypep type-1 type-2)))))
        (values (first answer) (second answer)))))
