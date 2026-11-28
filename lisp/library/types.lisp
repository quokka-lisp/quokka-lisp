;;;; types.lisp — the type system: `deftype`, `typep`, `type-of` (phase 3,
;;;; step A1, #124)
;;;;
;;;; A *type specifier* names a set of objects: `integer`, `(integer 0 9)`,
;;;; `(or string symbol)`, `(cons integer null)`, or a name a program gave
;;;; one with `deftype`. This file answers the two questions that ask about
;;;; one object: is OBJECT in the set (`typep`), and what is the most
;;;; specific standard name for OBJECT's type (`type-of`).
;;;;
;;;; `typep` looks at the object and the specifier directly: a symbol goes
;;;; through a table of predicates, a list through its head. It never builds
;;;; the canonical type objects that `subtypep` compares
;;;; (`type-algebra.lisp`), because `typep` is asked about one object at a time, often in a
;;;; loop, and an answer for one object needs none of that machinery.
;;;;
;;;; Quokka has one float format, the IEEE double, so `short-float`,
;;;; `single-float`, `double-float` and `long-float` all name the same set
;;;; (CLHS 12.1.4.1.1 allows an implementation fewer formats than four).
;;;;
;;;; Quokka has no base strings: its strings hold any character, so
;;;; `base-char` upgrades to `character` as an element type, and
;;;; `base-string` is the same type as `string`. In SBCL, which has base
;;;; strings, `"abc"` is not a `base-string`; here it is, as CLHS 15.1.2.1
;;;; says it must be for an implementation that upgrades `base-char` so.

;;; ------------------------------------------------------------------------
;;; deftype

;;; A `deftype` expander is a function of the form and the environment, as
;;; a macro's is, kept on the name's property list. `(upto 9)` is expanded
;;; by calling it with `(upto 9)`; the bare name `upto` with `(upto)`.
(defun %set-deftype (name expander)
  ;; A standard type's name is not the program's to give another meaning:
  ;; `(deftype integer () 'string)` was taken, and `(typep 1 'integer)` was
  ;; false from then on, for the whole world. SBCL refuses with a package
  ;; lock (#190).
  (when (gethash name %atomic-types)
    (error 'program-error "deftype cannot redefine a standard type" name))
  (setf (get name '%deftype) expander)
  name)

;;; CLHS `deftype`: an optional or keyword parameter with no default
;;; defaults to the symbol `*`, not to nil, so that `(deftype upto (&optional
;;; n) ...)` makes the bare `upto` mean "no upper bound". Only the top level
;;; of the lambda list is rewritten.
(defun %star-defaults (lambda-list)
  (let ((state nil) (result nil))
    (do ((rest lambda-list (cdr rest)))
        ((not (consp rest)) (append (nreverse result) rest))
      (let ((item (car rest)))
        (cond ((member item '(&optional &key)) (setq state t) (push item result))
              ((member item lambda-list-keywords) (setq state nil) (push item result))
              ((and state (symbolp item)) (push (list item ''*) result))
              ((and state (consp item) (null (cdr item))) (push (list (car item) ''*) result))
              (t (push item result)))))))

;;; The expander is made by `defmacro` under a hidden name, as
;;; `define-setf-expander` makes its own, so that the lambda list is taken
;;; apart by exactly the code a macro's is. Its body is in a block named
;;; after the type, as CLHS says (`(return-from name 'integer)` works), with
;;; the documentation string and declarations left in front of it.
(defmacro deftype (name lambda-list &body body)
  (let* ((hidden (gensym "DEFTYPE"))
         (documentation (if (and (stringp (car body)) (cdr body)) (list (car body)) nil))
         (body (if documentation (cdr body) body)))
    `(progn
       (defmacro ,hidden ,(%star-defaults lambda-list)
         ,@documentation
         ,@(%body-declarations body)
         (block ,name ,@(%body-forms body)))
       (%set-deftype ',name (%macro-function ',hidden)))))

;;; SPECIFIER with every `deftype` at its head expanded, until the head is
;;; something else. A chain of deftypes that never ends — `(deftype a () 'b)
;;; (deftype b () 'a)` — is an error after a hundred expansions; SBCL never
;;; returns from one.
(defun %expand-type (specifier)
  (do ((count 0 (+ count 1)))
      (nil)
    (let* ((head (if (consp specifier) (car specifier) specifier))
           (expander (and (symbolp head) (get head '%deftype))))
      (cond ((null expander) (return specifier))
            ((>= count 100) (error "a type specifier that expands without end" specifier))
            (t (setq specifier (funcall expander
                                        (if (consp specifier) specifier (list specifier))
                                        nil)))))))

;;; ------------------------------------------------------------------------
;;; Kinds of object the predicates elsewhere do not name

(defun %fixnump (object)
  (and (integerp object) (<= most-negative-fixnum object most-positive-fixnum)))

(defun %bignump (object) (and (integerp object) (not (%fixnump object))))

;;; SBCL's base characters are the ASCII ones.
(defun %base-char-p (object) (and (characterp object) (< (char-code object) 128)))

;;; A simple array is one that is not displaced, has no fill pointer and is
;;; not adjustable (CLHS 15.1.1.2). Quokka makes a header for every array of
;;; rank other than one, so a header alone does not decide it.
(defun %simple-array-p (object)
  (and (arrayp object)
       (or (not (%header-p object))
           (not (or (%array-header-slot object 2)
                    (%array-header-slot object 4)
                    (%array-header-slot object 5))))))

(defun %string-stream-p (object)
  (or (%string-input-stream-p object) (%string-output-stream-p object)))

;;; The library's own structures that stand for standard types. They are
;;; structures to `defstruct`, and to the program they are what the standard
;;; calls them.
(defun %standard-structure-name (object)
  (cond ((packagep object) 'package)
        ((readtablep object) 'readtable)
        ((%string-stream-p object) 'string-stream)
        ((streamp object) 'stream)
        (t nil)))

;;; Is OBJECT a structure of the type NAME names (or of a type including it)?
;;; Nil when NAME names no structure type; a structure of `:type list` or
;;; `vector` names no type.
(defun %structure-type-layout (name)
  (let ((layout (and (symbolp name) (%find-layout name))))
    (if (and layout (null (%layout-type layout))) layout nil)))

;;; ------------------------------------------------------------------------
;;; Atomic type specifiers

;;; The standard type names with no arguments, each with its predicate. A
;;; predicate that a later file defines (`packagep`, `streamp`, …) is called
;;; through a lambda, since `#'` would look for it now.
(defvar %atomic-types (make-hash-table :test 'eq))

(defun %define-atomic-types (pairs)
  (dolist (pair pairs) (setf (gethash (car pair) %atomic-types) (cdr pair))))

(%define-atomic-types
 (list (cons t (lambda (x) x t))
       (cons nil (lambda (x) x nil))
       (cons 'atom #'atom)
       (cons 'list #'listp)
       (cons 'cons #'consp)
       (cons 'null #'null)
       (cons 'symbol #'symbolp)
       (cons 'keyword #'keywordp)
       (cons 'boolean (lambda (x) (or (eq x t) (eq x nil))))
       (cons 'number #'realp)
       (cons 'real #'realp)
       (cons 'rational #'rationalp)
       (cons 'integer #'integerp)
       (cons 'ratio #'%ratiop)
       (cons 'fixnum #'%fixnump)
       (cons 'bignum #'%bignump)
       (cons 'bit (lambda (x) (or (eql x 0) (eql x 1))))
       (cons 'signed-byte #'integerp)
       (cons 'unsigned-byte (lambda (x) (and (integerp x) (>= x 0))))
       (cons 'float #'floatp)
       (cons 'short-float #'floatp)
       (cons 'single-float #'floatp)
       (cons 'double-float #'floatp)
       (cons 'long-float #'floatp)
       (cons 'complex (lambda (x) x nil))
       (cons 'character #'characterp)
       (cons 'base-char #'%base-char-p)
       (cons 'standard-char (lambda (x) (and (characterp x) (standard-char-p x))))
       (cons 'extended-char (lambda (x) (and (characterp x) (not (%base-char-p x)))))
       (cons 'array #'arrayp)
       (cons 'simple-array #'%simple-array-p)
       (cons 'vector #'vectorp)
       (cons 'simple-vector #'simple-vector-p)
       (cons 'string #'stringp)
       (cons 'simple-string #'simple-string-p)
       (cons 'base-string #'stringp)
       (cons 'simple-base-string #'simple-string-p)
       (cons 'bit-vector #'bit-vector-p)
       (cons 'simple-bit-vector #'simple-bit-vector-p)
       (cons 'sequence (lambda (x) (or (listp x) (vectorp x))))
       (cons 'function #'functionp)
       (cons 'compiled-function (lambda (x) (compiled-function-p x)))
       (cons 'hash-table #'hash-table-p)
       (cons 'package (lambda (x) (packagep x)))
       (cons 'readtable (lambda (x) (readtablep x)))
       (cons 'stream (lambda (x) (streamp x)))
       (cons 'string-stream (lambda (x) (%string-stream-p x)))
       (cons 'random-state (lambda (x) x nil))
       (cons 'pathname (lambda (x) x nil))
       (cons 'logical-pathname (lambda (x) x nil))
       (cons 'structure-object (lambda (x) (if (%structure-layout x) t nil)))
       (cons 'condition (lambda (x) (%condition-object-p x)))))

;;; Is OBJECT a condition? An instance of a condition class (step B, #125),
;;; or one of the machine's own condition objects, which a world with the
;;; library stops making in step B's third sub-step. `class-of` maps one of
;;; those to the class its type names, so a class test answers for both.
(defun %condition-object-p (object)
  (or (%conditionp object)
      (and (%instance-wrapper object) (%class-typep object (find-class 'condition)))))

;;; Is OBJECT of the type the symbol NAME names?
(defun %typep-atom (object name)
  (let ((predicate (gethash name %atomic-types)))
    (cond (predicate (if (funcall predicate object) t nil))
          ((%structure-type-layout name)
           (%structure-is-a object (%structure-type-layout name)))
          ((find-class name nil) (%class-typep object (find-class name nil)))
          (t (error "unknown type specifier" name)))))

;;; A class is a type: the objects whose class has it in its precedence list
;;; (CLHS 4.3.7). Every object has a class (`clos.lisp`), so this answers
;;; for a built-in class as well as for one `defclass` made.
(defun %class-typep (object class)
  (if (%member-eq class (%class-precedence-list (class-of object))) t nil))

;;; Is OBJECT a class? Classes are instances whose class is a metaclass,
;;; that is, a subclass of `class`.
(defun %class-object-p (object)
  (and (%instance-wrapper object)
       (member (find-class 'class) (%class-precedence-list (class-of object)))
       t))

;;; ------------------------------------------------------------------------
;;; Compound type specifiers

;;; ARGUMENTS, the arguments of a compound specifier, padded with `*` up to
;;; COUNT, which they must not exceed.
(defun %type-arguments (specifier count)
  (let ((arguments (cdr specifier)))
    (if (or (not (listp arguments)) (not (listp (last arguments))) (cdr (last arguments))
            (> (length arguments) count))
        (error "malformed type specifier" specifier))
    (append arguments (make-list (- count (length arguments)) :initial-element '*))))

;;; Is BOUND a valid bound of a numeric range: `*`, a real, or a list of one
;;; real (an exclusive bound)?
(defun %check-bound (bound specifier)
  (unless (or (eq bound '*) (realp bound)
              (and (consp bound) (realp (car bound)) (null (cdr bound))))
    (error "malformed type specifier" specifier)))

;;; Is the real X within LOW and HIGH, each a bound as above?
(defun %within-bounds (x low high)
  (and (cond ((eq low '*) t) ((consp low) (> x (car low))) (t (>= x low)))
       (cond ((eq high '*) t) ((consp high) (< x (car high))) (t (<= x high)))))

(defun %typep-range (object predicate specifier)
  (let* ((arguments (%type-arguments specifier 2))
         (low (first arguments)) (high (second arguments)))
    (%check-bound low specifier)
    (%check-bound high specifier)
    (and (funcall predicate object) (%within-bounds object low high))))

;;; Does an array's element type match ELEMENT-TYPE, a specifier or `*`?
;;; Element types are compared after upgrading (CLHS 15.1.2.1).
(defun %element-type-matches (array element-type)
  (or (eq element-type '*)
      (eq (upgraded-array-element-type element-type) (array-element-type array))))

;;; Do an array's dimensions match DIMENSIONS: `*`, a rank, or a list of
;;; sizes and `*`s?
(defun %dimensions-match (array dimensions specifier)
  (let ((actual (array-dimensions array)))
    (cond ((eq dimensions '*) t)
          ((integerp dimensions) (= (length actual) dimensions))
          ((listp dimensions)
           (and (= (length actual) (length dimensions))
                (every (lambda (want have)
                         (cond ((eq want '*) t)
                               ((integerp want) (= want have))
                               (t (error "malformed type specifier" specifier))))
                       dimensions actual)))
          (t (error "malformed type specifier" specifier)))))

;;; `(array element-type dimensions)` and its relatives. SIMPLE asks for a
;;; simple array.
(defun %typep-array (object element-type dimensions simple specifier)
  (and (arrayp object)
       (or (not simple) (%simple-array-p object))
       (%element-type-matches object element-type)
       (%dimensions-match object dimensions specifier)))

;;; One-dimensional array types whose argument is a size: `(vector et size)`
;;; is `(array et (size))`, and `*` for the size leaves it open.
(defun %size-dimensions (size) (list size))

(defun %typep-compound (object head specifier)
  (cond
    ((eq head 'and)
     (dolist (type (cdr specifier) t) (unless (%typep object type) (return nil))))
    ((eq head 'or)
     (dolist (type (cdr specifier) nil) (when (%typep object type) (return t))))
    ((eq head 'not)
     (not (%typep object (first (%type-arguments specifier 1)))))
    ((eq head 'member)
     (if (not (listp (last specifier))) (error "malformed type specifier" specifier))
     (if (cdr (last specifier)) (error "malformed type specifier" specifier))
     (if (member object (cdr specifier)) t nil))
    ((eq head 'eql) (eql object (first (%type-arguments specifier 1))))
    ((eq head 'satisfies)
     (let ((predicate (first (%type-arguments specifier 1))))
       ;; `(satisfies)` is padded with `*`, which is no predicate (#190).
       (unless (and (symbolp predicate) (cdr specifier) (not (eq predicate '*)))
         (error "malformed type specifier" specifier))
       (if (funcall predicate object) t nil)))
    ((eq head 'cons)
     (let ((arguments (%type-arguments specifier 2)))
       (and (consp object)
            (or (eq (first arguments) '*) (%typep (car object) (first arguments)))
            (or (eq (second arguments) '*) (%typep (cdr object) (second arguments))))))
    ((eq head 'integer) (%typep-range object #'integerp specifier))
    ((eq head 'rational) (%typep-range object #'rationalp specifier))
    ((eq head 'real) (%typep-range object #'realp specifier))
    ((member head '(float short-float single-float double-float long-float))
     (%typep-range object #'floatp specifier))
    ((eq head 'mod)
     (let ((n (first (%type-arguments specifier 1))))
       (unless (and (integerp n) (> n 0)) (error "malformed type specifier" specifier))
       (and (integerp object) (<= 0 object) (< object n))))
    ((member head '(signed-byte unsigned-byte))
     (let ((size (first (%type-arguments specifier 1))))
       (unless (or (eq size '*) (and (integerp size) (> size 0)))
         (error "malformed type specifier" specifier))
       (and (integerp object)
            (if (eq head 'unsigned-byte)
                (and (>= object 0) (or (eq size '*) (< object (ash 1 size))))
                (or (eq size '*)
                    (and (>= object (- (ash 1 (- size 1)))) (< object (ash 1 (- size 1)))))))))
    ((member head '(array simple-array))
     (let ((arguments (%type-arguments specifier 2)))
       (%typep-array object (first arguments) (second arguments)
                     (eq head 'simple-array) specifier)))
    ((eq head 'vector)
     (let ((arguments (%type-arguments specifier 2)))
       (%typep-array object (first arguments) (%size-dimensions (second arguments))
                     nil specifier)))
    ((member head '(simple-vector string simple-string base-string simple-base-string
                    bit-vector simple-bit-vector))
     (let ((size (first (%type-arguments specifier 1))))
       (%typep-array object
                     (cond ((member head '(string simple-string base-string simple-base-string))
                            'character)
                           ((member head '(bit-vector simple-bit-vector)) 'bit)
                           (t t))
                     (%size-dimensions size)
                     (member head '(simple-vector simple-string simple-base-string
                                    simple-bit-vector))
                     specifier)))
    ((eq head 'complex) (%type-arguments specifier 1) nil)
    ;; CLHS says `typep` refuses a function type. The pinned SBCL refuses
    ;; it for any object; Quokka refuses it for a function and answers nil
    ;; for anything else, which no function type could be true of.
    ((eq head 'function)
     (if (functionp object)
         (error "a function type specifier cannot be used with typep" specifier)
         nil))
    ((eq head 'values) (error "a values type specifier cannot be used with typep" specifier))
    (t (error "unknown type specifier" specifier))))

;;; Is OBJECT of TYPE?
(defun %typep (object type)
  (let ((type (%expand-type type)))
    (cond ((consp type) (%typep-compound object (car type) type))
          ((symbolp type) (%typep-atom object type))
          ((%class-object-p type) (%class-typep object type))
          (t (error "malformed type specifier" type)))))

(defun typep (object type-specifier &optional environment)
  environment
  (%typep object type-specifier))

;;; ------------------------------------------------------------------------
;;; type-of

;;; The type of an array, as SBCL writes it: `(simple-array character (3))`
;;; for a simple string, `(and (vector t 3) (not simple-array))` for an
;;; array that is not simple.
(defun %array-type-of (array)
  (let ((element-type (array-element-type array))
        (dimensions (array-dimensions array)))
    (cond ((and (simple-vector-p array)) (list 'simple-vector (length array)))
          ((simple-bit-vector-p array) (list 'simple-bit-vector (length array)))
          ((%simple-array-p array) (list 'simple-array element-type dimensions))
          ((= (length dimensions) 1)
           (list 'and (list 'vector element-type (car dimensions)) '(not simple-array)))
          (t (list 'and (list 'array element-type dimensions) '(not simple-array))))))

(defun type-of (object)
  (cond ((null object) 'null)
        ((eq object t) 'boolean)
        ((keywordp object) 'keyword)
        ((symbolp object) 'symbol)
        ((consp object) 'cons)
        ((integerp object)
         (cond ((not (%fixnump object)) 'bignum)
               ((>= object 0) (list 'integer 0 most-positive-fixnum))
               (t 'fixnum)))
        ((%ratiop object) 'ratio)
        ((floatp object) 'double-float)
        ((characterp object)
         (cond ((standard-char-p object) 'standard-char)
               ((%base-char-p object) 'base-char)
               (t 'character)))
        ((arrayp object) (%array-type-of object))
        ;; An instance's type is its class's name, if that name still names
        ;; the class; otherwise the class itself. A generic function is an
        ;; instance and a function, and its type is its class's name.
        ((%instance-wrapper object)
         (let ((class (class-of object)))
           (if (and (%class-name class) (eq (find-class (%class-name class) nil) class))
               (%class-name class)
               class)))
        ((functionp object) (if (compiled-function-p object) 'compiled-function 'function))
        ((hash-table-p object) 'hash-table)
        ((%conditionp object) (%condition-type object))
        ((%standard-structure-name object))
        ((%structure-layout object) (%layout-name (%structure-layout object)))
        (t t)))

;;; ------------------------------------------------------------------------
;;; typecase, etypecase, ctypecase, check-type

;;; A clause `(type form...)` becomes a `cond` clause testing `typep`; `t`
;;; or `otherwise` matches anything. The leading nil makes a clause with no
;;; forms return nil, as CLHS says, where a bare `cond` clause would return
;;; its test.
(defun %typecase-clauses (key clauses)
  (mapcar (lambda (clause)
            (if (member (car clause) '(t otherwise))
                `(t nil ,@(cdr clause))
                `((typep ,key ',(car clause)) nil ,@(cdr clause))))
          clauses))

(defmacro typecase (keyform &body clauses)
  (let ((key (gensym "KEY")))
    `(let ((,key ,keyform))
       (cond ,@(%typecase-clauses key clauses)))))

;;; `etypecase` and `ctypecase` have no catch-all clause: `otherwise` is a
;;; type name there like any other, as SBCL takes it (and no type), and `t`
;;; the type of every object.
(defun %etypecase-clauses (key clauses)
  (mapcar (lambda (clause) `((typep ,key ',(car clause)) nil ,@(cdr clause))) clauses))

(defmacro etypecase (keyform &body clauses)
  (let ((key (gensym "KEY")))
    `(let ((,key ,keyform))
       (cond ,@(%etypecase-clauses key clauses)
             (t (error 'type-error :datum ,key :expected-type '(or ,@(mapcar #'car clauses))))))))

;;; `ctypecase` and `check-type` are in `conditions.lisp`, with the
;;; `store-value` restart that makes their errors correctable (phase 3 step B).

;;; ------------------------------------------------------------------------
;;; The sequence functions, through deftype

;;; The standard sequence types' names: `coerce` and the sequence functions make a sequence of one of
;;; these without asking `subtypep`, since `format` and the reader call it
;;; for every string they build.
(defvar %sequence-type-heads
  '(list cons null string simple-string base-string simple-base-string vector
    simple-vector array simple-array bit-vector simple-bit-vector))


;;; `make-sequence`, `concatenate`, `map`, `merge` and `coerce` were written
;;; in `sequences.lisp` before there was a type system, and know the
;;; standard names of sequence types only. Each of their helpers is wrapped
;;; here to expand the type first, so that a name `deftype` gave a sequence
;;; type works too.
;;; A result type the old code does not know by its head, such as `(or
;;; (vector t 10) (vector t 5))`, is placed by `subtypep` instead.
(let ((plain #'%kind-of-type))
  (defun %kind-of-type (type)
    (let ((type (%expand-type type)))
      (if (member (if (consp type) (car type) type) %sequence-type-heads)
          (funcall plain type)
          ;; A union of vectors of different element types, such as `(or
          ;; (vector bit) (vector t))`, says no one kind of vector to make:
          ;; an error, as in SBCL.
          (cond ((subtypep type 'list) :list)
                ((subtypep type 'string) :string)
                ((subtypep type 'bit-vector) :bit-vector)
                ((subtypep type '(vector t)) :vector)
                (t (error 'type-error "not a sequence type of one kind" type)))))))

(let ((plain #'%length-of-type))
  (defun %length-of-type (type) (funcall plain (%expand-type type))))

;;; A type the old code does not know by its head is checked against the
;;; sequence made: `(or (vector t 5) (vector t 10))` cannot hold six.
(let ((plain #'%of-type))
  (defun %of-type (type items)
    (let* ((type (%expand-type type))
           (result (funcall plain type items)))
      (if (or (member (if (consp type) (car type) type) %sequence-type-heads)
              (typep result type))
          result
          (error 'type-error "a sequence of that type cannot have these elements"
                 (list type (length items)))))))

;;; ------------------------------------------------------------------------
;;; coerce

(defun %coerce-error (object type)
  (error 'type-error :datum object :expected-type type))

(defun coerce (object result-type)
  (let* ((type (%expand-type result-type))
         (head (if (consp type) (car type) type)))
    (cond ((eq type t) object)
          ((member head '(float short-float single-float double-float long-float))
           (cond ((floatp object) object)
                 ((realp object) (float object))
                 (t (%coerce-error object result-type))))
          ((and (member head %sequence-type-heads)
                (or (listp object) (vectorp object)))
           (if (typep object type)
               object
               (%of-type type (%range-list object 0 nil))))
          ((typep object type) object)
          ((eq type 'character)
           (if (or (characterp object)
                   (and (stringp object) (= (length object) 1))
                   (and (symbolp object) (= (length (symbol-name object)) 1)))
               (character object)
               (%coerce-error object result-type)))
          ((eq type 'function)
           (cond ((and (symbolp object) (fboundp object) (not (macro-function object))
                       (not (special-operator-p object)))
                  (fdefinition object))
                 ((and (consp object) (eq (car object) 'lambda)) (compile nil object))
                 (t (%coerce-error object result-type))))
          ((eq head 'complex) (error "complex numbers are not implemented" object))
          (t (%coerce-error object result-type)))))
