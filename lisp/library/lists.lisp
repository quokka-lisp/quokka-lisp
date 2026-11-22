;;;; The basics: equality, iteration and control, and the list functions
;;;; (#54, step J1 of #39).
;;;;
;;;; Step J was "sequences", and taking stock showed the gap is wider: a world
;;;; had no `eql`, `equal`, `dolist`, `case`, `mapcar` or `assoc`. This file is
;;;; the first of three parts (decision 1): CLHS chapters 5 and 6 for the
;;;; macros, chapter 14 for the lists, and the equality predicates. Sequences
;;;; (chapter 17) and strings (chapter 16) follow in their own files.
;;;;
;;;; Decided in #54 (Carsten): `eql` is defined now (decision 2), and the
;;;; prelude's narrower functions — `member` here — are redefined with CL's
;;;; full signatures (decision 3). Calls are late-bound, so that changes what
;;;; the running compiler *calls* after a rebuild and not the code it is made
;;;; of, which is what the fixpoint compares. No macro of the prelude's is
;;;; redefined: a redefined macro would change what the compiler's source
;;;; expands into.

;;; ------------------------------------------------------------------------
;;; Equality

;;; `eql` is `eq`, plus numbers and characters compared by value — which
;;; matters only once a number can be *boxed*, two equal numbers being two
;;; heap objects. None is yet: every integer is a fixnum and every character
;;; an immediate, so `eq` is exactly right for everything Quokka can make.
;;; Bignums (step L) and floats (M) redefine this one function, and every
;;; `:test` that defaults to it becomes right with them (decision 2).
(defun eql (a b) (eq a b))

;;; Structure: conses by their parts, strings by their characters, anything
;;; else by `eql`. Down the `cdr` by iteration, so a long list costs no depth.
(defun equal (a b)
  (cond ((eql a b) t)
        ((consp a)
         (and (consp b)
              (equal (car a) (car b))
              (equal (cdr a) (cdr b))))
        ((stringp a) (and (stringp b) (string= a b)))
        (t nil)))

;;; Looser still: characters ignoring case, numbers by `=`, and vectors and
;;; strings element by element. A structure is a vector in Quokka, so
;;; structures compare slot by slot, as CL's do. Hash tables are not compared
;;; by content yet.
(defun equalp (a b)
  (cond ((eq a b) t)
        ((characterp a) (and (characterp b) (eq (char-upcase a) (char-upcase b))))
        ((integerp a) (and (integerp b) (= a b)))
        ((consp a)
         (and (consp b)
              (equalp (car a) (car b))
              (equalp (cdr a) (cdr b))))
        ((or (vectorp a) (stringp a))
         (and (or (vectorp b) (stringp b))
              (= (length a) (length b))
              (labels ((walk (i)
                         (cond ((= i (length a)) t)
                               ((equalp (aref a i) (aref b i)) (walk (+ i 1)))
                               (t nil))))
                (walk 0))))
        (t nil)))

;;; ------------------------------------------------------------------------
;;; Functions about functions (CLHS 5)

(defun identity (object) object)

(defun constantly (value)
  (lambda (&rest arguments) arguments value))

(defun complement (function)
  (lambda (&rest arguments) (not (apply function arguments))))

;;; ------------------------------------------------------------------------
;;; Control (CLHS 5)

;;; `(prog1 first form...)`: FIRST's value, after the rest have run.
(defmacro prog1 (first &body forms)
  (let ((value (gensym "VALUE")))
    `(let ((,value ,first)) ,@forms ,value)))

(defmacro prog2 (first second &body forms)
  `(progn ,first (prog1 ,second ,@forms)))

;;; `(case key clause...)`: the first clause whose keys include KEY's value,
;;; by `eql`. A clause's keys are a list, or one atom; `t` and `otherwise`
;;; match anything, and must come last.
(defmacro case (keyform &body clauses)
  (let ((key (gensym "KEY")))
    `(let ((,key ,keyform))
       (cond ,@(%case-clauses key clauses nil)))))

;;; `ecase`: as `case`, and an error if nothing matches.
(defmacro ecase (keyform &body clauses)
  (let ((key (gensym "KEY")))
    `(let ((,key ,keyform))
       (cond ,@(%case-clauses key clauses nil)
             (t (error "ECASE: no clause matches" ,key))))))

(defun %case-clauses (key clauses acc)
  (if (null clauses)
      (reverse acc)
      (let ((keys (car (car clauses)))
            (body (cdr (car clauses))))
        (%case-clauses key (cdr clauses)
                       (cons (list (cond ((or (eq keys t) (eq keys 'otherwise)) t)
                                         ((listp keys) (%case-test key keys))
                                         (t (list 'eql key (list 'quote keys))))
                                   (cons 'progn body))
                             acc)))))

(defun %case-test (key keys)
  (if (null keys)
      nil
      (cons 'or (%case-tests key keys))))

(defun %case-tests (key keys)
  (if (null keys)
      nil
      (cons (list 'eql key (list 'quote (car keys))) (%case-tests key (cdr keys)))))

;;; `(destructuring-bind lambda-list expression body...)`: take EXPRESSION's
;;; value apart as a macro takes its arguments apart. The function is made by
;;; the compiler from the same rewrite `defmacro` uses.
(defmacro destructuring-bind (lambda-list expression &body body)
  `(apply (%destructuring-lambda ,lambda-list ,@body) ,expression))

;;; ------------------------------------------------------------------------
;;; Iteration (CLHS 6)
;;;
;;; Every loop is a `block nil` around a `tagbody`, which is what `return`
;;; leaves and what a `go` inside the body may jump within: the body of each
;;; is an implicit `tagbody`, so it may hold tags of its own. The variables
;;; are bound once and assigned on each step, which CLHS allows; a closure
;;; that captures one sees its last value.

(defmacro return (&optional value)
  `(return-from nil ,value))

;;; `(psetq var form ...)`: every form first, then every assignment. `psetf`
;;; on variables is exactly that, and is right for symbol macros too.
(defmacro psetq (&rest pairs)
  `(psetf ,@pairs))

(defmacro dolist ((var list-form &optional result) &body body)
  (let ((tail (gensym "TAIL"))
        (top (gensym "TOP"))
        (end (gensym "END")))
    `(block nil
       (let ((,tail ,list-form)
             (,var nil))
         (tagbody
          ,top
            (if (endp ,tail) (go ,end))
            (setq ,var (car ,tail))
            ,@body
            (setq ,tail (cdr ,tail))
            (go ,top)
          ,end)
         (setq ,var nil)
         ,result))))

(defmacro dotimes ((var count-form &optional result) &body body)
  (let ((count (gensym "COUNT"))
        (top (gensym "TOP"))
        (end (gensym "END")))
    `(block nil
       (let ((,count ,count-form)
             (,var 0))
         (tagbody
          ,top
            (if (>= ,var ,count) (go ,end))
            ,@body
            (setq ,var (+ ,var 1))
            (go ,top)
          ,end)
         ,result))))

;;; `(do ((var init step)...) (end-test result...) body...)`: all inits,
;;; then until END-TEST: the body, and every step at once (`psetq`). `do*`
;;; binds and steps in order instead.
(defmacro do (bindings (end-test &rest result) &body body)
  (%do-expansion 'let 'psetq bindings end-test result body))

(defmacro do* (bindings (end-test &rest result) &body body)
  (%do-expansion 'let* 'setq bindings end-test result body))

(defun %do-expansion (binder stepper bindings end-test result body)
  (let ((top (gensym "TOP"))
        (end (gensym "END")))
    `(block nil
       (,binder ,(%do-inits bindings)
        (tagbody
         ,top
           (if ,end-test (go ,end))
           ,@body
           (,stepper ,@(%do-steps bindings))
           (go ,top)
         ,end)
        (progn ,@result)))))

(defun %do-inits (bindings)
  (if (null bindings)
      nil
      (let ((b (car bindings)))
        (cons (if (consp b) (list (car b) (car (cdr b))) (list b nil))
              (%do-inits (cdr bindings))))))

;;; `var step` for each binding that has a step.
(defun %do-steps (bindings)
  (cond ((null bindings) nil)
        ((and (consp (car bindings)) (consp (cdr (cdr (car bindings)))))
         (cons (car (car bindings))
               (cons (car (cdr (cdr (car bindings))))
                     (%do-steps (cdr bindings)))))
        (t (%do-steps (cdr bindings)))))

;;; ------------------------------------------------------------------------
;;; Tests: what `:key`, `:test` and `:test-not` mean, once
;;;
;;; Every function below that compares takes these three keywords. They
;;; become one two-argument predicate, applied as CL says: the item first,
;;; then the element's key.

(defun %test (test test-not)
  (cond ((and test test-not) (error "both :TEST and :TEST-NOT given"))
        (test test)
        (test-not (lambda (a b) (not (funcall test-not a b))))
        (t #'eql)))

(defun %key (key element)
  (if key (funcall key element) element))

;;; ------------------------------------------------------------------------
;;; Conses (CLHS 14)

(defun atom (object) (not (consp object)))

(defun caaar (x) (car (car (car x))))
(defun caadr (x) (car (car (cdr x))))
(defun cadar (x) (car (cdr (car x))))
(defun cdaar (x) (cdr (car (car x))))
(defun cdadr (x) (cdr (car (cdr x))))
(defun cddar (x) (cdr (cdr (car x))))
(defun cdddr (x) (cdr (cdr (cdr x))))
(defun cadddr (x) (car (cdddr x)))
(defun cddddr (x) (cdr (cdddr x)))

(defun fourth (list) (car (cdddr list)))
(defun fifth (list) (car (cddddr list)))
(defun sixth (list) (car (cdr (cddddr list))))
(defun seventh (list) (car (cddr (cddddr list))))
(defun eighth (list) (car (cdddr (cddddr list))))
(defun ninth (list) (car (cddddr (cddddr list))))
(defun tenth (list) (car (cdr (cddddr (cddddr list)))))

;;; Each settable, as CLHS 5.1.2.2 says: `(setf (fourth x) v)` stores into
;;; the right cons. Long `defsetf`s, written by a macro of their own.
(defmacro %defsetf-car-of (name path)
  `(defsetf ,name (x) (new) (list 'progn (list 'rplaca ,path new) new)))

(defmacro %defsetf-cdr-of (name path)
  `(defsetf ,name (x) (new) (list 'progn (list 'rplacd ,path new) new)))

(%defsetf-car-of caaar (list 'car (list 'car x)))
(%defsetf-car-of caadr (list 'car (list 'cdr x)))
(%defsetf-cdr-of cdaar (list 'car (list 'car x)))
(%defsetf-cdr-of cdadr (list 'car (list 'cdr x)))
(%defsetf-cdr-of cddar (list 'cdr (list 'car x)))
(%defsetf-cdr-of cdddr (list 'cddr x))
(%defsetf-cdr-of cddddr (list 'cdddr x))
(%defsetf-car-of cadar (list 'cdr (list 'car x)))
(%defsetf-car-of cadddr (list 'cdddr x))
(%defsetf-car-of fourth (list 'cdddr x))
(%defsetf-car-of fifth (list 'cddddr x))
(%defsetf-car-of sixth (list 'nthcdr 5 x))
(%defsetf-car-of seventh (list 'nthcdr 6 x))
(%defsetf-car-of eighth (list 'nthcdr 7 x))
(%defsetf-car-of ninth (list 'nthcdr 8 x))
(%defsetf-car-of tenth (list 'nthcdr 9 x))

(defun list* (first &rest rest)
  (if (null rest)
      first
      (cons first (apply #'list* rest))))

(defun make-list (size &key initial-element)
  (let ((list nil))
    (dotimes (i size list)
      (setq list (cons initial-element list)))))

(defun nthcdr (n list)
  (do ((i 0 (+ i 1))
       (tail list (cdr tail)))
      ((= i n) tail)))

;;; The length, or nil for a circular list — found by a pointer that moves
;;; twice as fast as the other (Floyd's cycle-finding, "tortoise and hare").
(defun list-length (list)
  (do ((n 0 (+ n 2))
       (fast list (cddr fast))
       (slow list (cdr slow)))
      (nil)
    (cond ((endp fast) (return n))
          ((endp (cdr fast)) (return (+ n 1)))
          ((and (eq fast slow) (> n 0)) (return nil)))))

(defun last (list &optional (n 1))
  (let ((count (length list)))
    (nthcdr (if (> count n) (- count n) 0) list)))

(defun butlast (list &optional (n 1))
  (let ((keep (- (length list) n)))
    (do ((i 0 (+ i 1))
         (tail list (cdr tail))
         (out nil (cons (car tail) out)))
        ((>= i keep) (reverse out)))))

(defun nbutlast (list &optional (n 1))
  (let ((keep (- (length list) n)))
    (if (<= keep 0)
        nil
        (progn (rplacd (nthcdr (- keep 1) list) nil) list))))

(defun copy-list (list)
  (if (atom list)
      list
      (let ((head (cons (car list) nil)))
        (do ((from (cdr list) (cdr from))
             (to head (cdr to)))
            ((atom from) (rplacd to from) head)
          (rplacd to (cons (car from) nil))))))

(defun copy-alist (alist)
  (let ((out nil))
    (dolist (pair alist (reverse out))
      (push (if (consp pair) (cons (car pair) (cdr pair)) pair) out))))

(defun copy-tree (tree)
  (if (consp tree)
      (cons (copy-tree (car tree)) (copy-tree (cdr tree)))
      tree))

(defun tree-equal (a b &key test test-not)
  (let ((same (%test test test-not)))
    (labels ((walk (a b)
               (cond ((and (consp a) (consp b))
                      (and (walk (car a) (car b)) (walk (cdr a) (cdr b))))
                     ((or (consp a) (consp b)) nil)
                     (t (funcall same a b)))))
      (walk a b))))

;;; Destructive append: each non-empty list's last cons is pointed at the
;;; next. The lists themselves are the result; nothing is copied.
(defun nconc (&rest lists)
  (let ((result nil)
        (last-cons nil))
    (dolist (list lists result)
      (cond ((null list))
            ((null result) (setq result list) (setq last-cons (last list)))
            ((atom list) (rplacd last-cons list))
            (t (rplacd last-cons list) (setq last-cons (last list)))))))

(defun revappend (list tail)
  (do ((rest list (cdr rest))
       (out tail (cons (car rest) out)))
      ((endp rest) out)))

(defun nreconc (list tail) (revappend list tail))

(defun tailp (object list)
  (do ((tail list (cdr tail)))
      ((atom tail) (eql object tail))
    (if (eql object tail) (return t))))

;;; LIST up to OBJECT, copied; a dotted end that OBJECT is not stays on.
(defun ldiff (list object)
  (do ((tail list (cdr tail))
       (out nil (cons (car tail) out)))
      ((or (eql tail object) (atom tail))
       (revappend out (if (eql tail object) nil tail)))))

;;; ------------------------------------------------------------------------
;;; Mapping

;;; The cars of LISTS, and their cdrs, a step of a walk down all of them.
(defun %cars (lists)
  (let ((out nil))
    (dolist (list lists (reverse out))
      (push (car list) out))))

(defun %cdrs (lists)
  (let ((out nil))
    (dolist (list lists (reverse out))
      (push (cdr list) out))))

(defun %any-ended (lists)
  (dolist (list lists nil)
    (if (endp list) (return t))))

;;; One walk for all six: over the elements or over the tails, collecting,
;;; concatenating or discarding.
(defun %map-lists (function lists tails how)
  (let ((out nil))
    (do ((rest lists (%cdrs rest)))
        ((%any-ended rest)
         (cond ((eq how :collect) (reverse out))
               ((eq how :concatenate) (apply #'nconc (reverse out)))
               (t (car lists))))
      (let ((value (apply function (if tails rest (%cars rest)))))
        (if (not (eq how :discard)) (push value out))))))

(defun mapcar (function list &rest more)
  (%map-lists function (cons list more) nil :collect))
(defun mapc (function list &rest more)
  (%map-lists function (cons list more) nil :discard))
(defun mapcan (function list &rest more)
  (%map-lists function (cons list more) nil :concatenate))
(defun maplist (function list &rest more)
  (%map-lists function (cons list more) t :collect))
(defun mapl (function list &rest more)
  (%map-lists function (cons list more) t :discard))
(defun mapcon (function list &rest more)
  (%map-lists function (cons list more) t :concatenate))

;;; ------------------------------------------------------------------------
;;; Membership and association

;;; The prelude's `member` compares with `eq` and takes no keywords; this is
;;; CL's (decision 3). On the compiler's own two-argument calls it answers as
;;; the prelude's did, since `eql` is `eq`.
(defun member (item list &key key test test-not)
  (let ((same (%test test test-not)))
    (do ((tail list (cdr tail)))
        ((endp tail) nil)
      (if (funcall same item (%key key (car tail))) (return tail)))))

(defun member-if (predicate list &key key)
  (do ((tail list (cdr tail)))
      ((endp tail) nil)
    (if (funcall predicate (%key key (car tail))) (return tail))))

(defun member-if-not (predicate list &key key)
  (member-if (complement predicate) list :key key))

(defun adjoin (item list &key key test test-not)
  (if (member (%key key item) list :key key :test test :test-not test-not)
      list
      (cons item list)))

(defun acons (key datum alist) (cons (cons key datum) alist))

(defun pairlis (keys data &optional alist)
  (do ((k keys (cdr k))
       (d data (cdr d))
       (out alist (acons (car k) (car d) out)))
      ((or (endp k) (endp d)) out)))

;;; A nil in an alist is skipped, as CLHS says.
(defun assoc (item alist &key key test test-not)
  (let ((same (%test test test-not)))
    (dolist (pair alist nil)
      (if (and pair (funcall same item (%key key (car pair)))) (return pair)))))

(defun assoc-if (predicate alist &key key)
  (dolist (pair alist nil)
    (if (and pair (funcall predicate (%key key (car pair)))) (return pair))))

(defun assoc-if-not (predicate alist &key key)
  (assoc-if (complement predicate) alist :key key))

(defun rassoc (item alist &key key test test-not)
  (let ((same (%test test test-not)))
    (dolist (pair alist nil)
      (if (and pair (funcall same item (%key key (cdr pair)))) (return pair)))))

(defun rassoc-if (predicate alist &key key)
  (dolist (pair alist nil)
    (if (and pair (funcall predicate (%key key (cdr pair)))) (return pair))))

(defun rassoc-if-not (predicate alist &key key)
  (rassoc-if (complement predicate) alist :key key))

;;; ------------------------------------------------------------------------
;;; Trees

(defun subst (new old tree &key key test test-not)
  (let ((same (%test test test-not)))
    (labels ((walk (tree)
               (cond ((funcall same old (%key key tree)) new)
                     ((atom tree) tree)
                     (t (cons (walk (car tree)) (walk (cdr tree)))))))
      (walk tree))))

(defun subst-if (new predicate tree &key key)
  (labels ((walk (tree)
             (cond ((funcall predicate (%key key tree)) new)
                   ((atom tree) tree)
                   (t (cons (walk (car tree)) (walk (cdr tree)))))))
    (walk tree)))

(defun subst-if-not (new predicate tree &key key)
  (subst-if new (complement predicate) tree :key key))

(defun nsubst (new old tree &key key test test-not)
  (subst new old tree :key key :test test :test-not test-not))

(defun sublis (alist tree &key key test test-not)
  (labels ((walk (tree)
             (let ((pair (assoc (%key key tree) alist :test test :test-not test-not)))
               (cond (pair (cdr pair))
                     ((atom tree) tree)
                     (t (cons (walk (car tree)) (walk (cdr tree))))))))
    (walk tree)))

(defun nsublis (alist tree &key key test test-not)
  (sublis alist tree :key key :test test :test-not test-not))

;;; ------------------------------------------------------------------------
;;; Lists as sets
;;;
;;; CLHS leaves the order of the result unspecified, so these keep whatever
;;; order is simplest and the tests compare as sets.

(defun %member-by (element list key same)
  (do ((tail list (cdr tail)))
      ((endp tail) nil)
    (if (funcall same (%key key element) (%key key (car tail))) (return tail))))

(defun union (list-1 list-2 &key key test test-not)
  (let ((same (%test test test-not))
        (out list-2))
    (dolist (element list-1 out)
      (if (not (%member-by element list-2 key same)) (push element out)))))

(defun nunion (list-1 list-2 &key key test test-not)
  (union list-1 list-2 :key key :test test :test-not test-not))

(defun intersection (list-1 list-2 &key key test test-not)
  (let ((same (%test test test-not))
        (out nil))
    (dolist (element list-1 (reverse out))
      (if (%member-by element list-2 key same) (push element out)))))

(defun nintersection (list-1 list-2 &key key test test-not)
  (intersection list-1 list-2 :key key :test test :test-not test-not))

(defun set-difference (list-1 list-2 &key key test test-not)
  (let ((same (%test test test-not))
        (out nil))
    (dolist (element list-1 (reverse out))
      (if (not (%member-by element list-2 key same)) (push element out)))))

(defun nset-difference (list-1 list-2 &key key test test-not)
  (set-difference list-1 list-2 :key key :test test :test-not test-not))

(defun set-exclusive-or (list-1 list-2 &key key test test-not)
  (append (set-difference list-1 list-2 :key key :test test :test-not test-not)
          (set-difference list-2 list-1 :key key :test test :test-not test-not)))

(defun subsetp (list-1 list-2 &key key test test-not)
  (let ((same (%test test test-not)))
    (dolist (element list-1 t)
      (if (not (%member-by element list-2 key same)) (return nil)))))
