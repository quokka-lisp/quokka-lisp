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
;;; matters once a number can be *boxed*, two equal numbers being two heap
;;; objects. A fixnum and a character are immediate, so `eq` is right for
;;; them; a bignum (step L, #16) is boxed, and two are compared by value.
;;; Every `:test` that defaults to `eql` became right for bignums with this
;;; one change (decision 2); floats (M) will add their own case.
;;; A double (step M) is the same as another with the same bits — so 0.0
;;; and -0.0 are not `eql`, though they are `=`.
;;; A ratio (#59) is canonical, so two are `eql` when their parts are.
(defun eql (a b)
  (or (eq a b)
      (and (%bignump a) (%bignump b) (eq (%integer-compare a b) 0))
      (%double-eql a b)
      (and (%ratiop a) (%ratiop b)
           (eql (%ratio-numerator a) (%ratio-numerator b))
           (eql (%ratio-denominator a) (%ratio-denominator b)))))

;;; Structure: conses by their parts, strings by their characters, anything
;;; else by `eql`. Down the `cdr` by iteration, so a long list costs no depth.
(defun equal (a b)
  (cond ((eql a b) t)
        ((consp a)
         (and (consp b)
              (equal (car a) (car b))
              (equal (cdr a) (cdr b))))
        ((stringp a) (and (stringp b) (string= a b)))
        ;; Bit vectors too, element by element (#67). A `labels` loop, as
        ;; `equalp`'s below: `dotimes` is defined further down this file.
        ((bit-vector-p a)
         (and (bit-vector-p b)
              (= (length a) (length b))
              (labels ((walk (i)
                         (cond ((= i (length a)) t)
                               ((eql (aref a i) (aref b i)) (walk (+ i 1)))
                               (t nil))))
                (walk 0))))
        (t nil)))

;;; Looser still: characters ignoring case, numbers by `=`, and vectors and
;;; strings element by element, and structures slot by slot (`structures.lisp`,
;;; which loads later). Hash tables are not compared
;;; by content yet.
(defun equalp (a b)
  (cond ((eq a b) t)
        ((characterp a) (and (characterp b) (eq (char-upcase a) (char-upcase b))))
        ((numberp a) (and (numberp b) (= a b)))
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
        ;; Arrays of several dimensions: the same dimensions, and the
        ;; elements in row-major order (#67).
        ((arrayp a)
         (and (arrayp b)
              (equal (array-dimensions a) (array-dimensions b))
              (labels ((walk (i)
                         (cond ((= i (array-total-size a)) t)
                               ((equalp (row-major-aref a i) (row-major-aref b i)) (walk (+ i 1)))
                               (t nil))))
                (walk 0))))
        ;; Two structures of one type, slot by slot (#73).
        ((%structure-layout a) (%structure-equalp a b))
        ;; Two hash tables with the same test and count, in which every key
        ;; of one has an `equalp` value in the other (CLHS `equalp`, #79).
        ((hash-table-p a)
         (and (hash-table-p b)
              (= (hash-table-count a) (hash-table-count b))
              (eq (hash-table-test a) (hash-table-test b))
              (%hash-tables-equalp a b)))
        (t nil)))

(defun %hash-tables-equalp (a b)
  (block compare
    (maphash (lambda (key value)
               (multiple-value-bind (other found) (gethash key b)
                 (if (not (and found (equalp value other))) (return-from compare nil))))
             a)
    t))

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

;;; `ecase`: as `case`, and a `type-error` if nothing matches (CLHS; SBCL's
;;; `case-failure` is one).
(defmacro ecase (keyform &body clauses)
  (let ((key (gensym "KEY")))
    `(let ((,key ,keyform))
       (cond ,@(%case-clauses key (%literal-keys clauses) nil)
             (t (error 'type-error :datum ,key
                                   :expected-type '(member ,@(%ecase-keys clauses))))))))

;;; An `ecase` or `ccase` has no catch-all clause: `t` and `otherwise` are
;;; keys there like any other symbol, as SBCL takes them, so `(ecase 1 (t
;;; nil))` signals. `%case-clauses` takes them as `case`'s catch-alls, so
;;; they are put in a list of one first.
(defun %literal-keys (clauses)
  (if (null clauses)
      nil
      (cons (if (member (car (car clauses)) '(t otherwise))
                (cons (list (car (car clauses))) (cdr (car clauses)))
                (car clauses))
            (%literal-keys (cdr clauses)))))

;;; Every key of CLAUSES, for the type an `ecase` error expects. Plain
;;; recursion: `dolist` is defined further down this file.
(defun %ecase-keys (clauses)
  (if (null clauses)
      nil
      (append (if (listp (car (car clauses))) (car (car clauses)) (list (car (car clauses))))
              (%ecase-keys (cdr clauses)))))

;;; `ccase` is in `conditions.lisp`, with the `store-value` restart that
;;; makes its error correctable (phase 3 step B).

(defun %case-clauses (key clauses acc)
  (if (null clauses)
      (reverse acc)
      (let ((keys (car (car clauses)))
            (body (cdr (car clauses))))
        (%case-clauses key (cdr clauses)
                       (cons (list (cond ((or (eq keys t) (eq keys 'otherwise)) t)
                                         ((listp keys) (%case-test key keys))
                                         (t (%case-compare key keys)))
                                   (cons 'progn body))
                             acc)))))

;;; One key's test. `case` compares with `eql`, and `eql` is `eq` for every
;;; key but a number that is boxed — a bignum, a float, a ratio. The key is
;;; in the source, so that is known now, and a symbol, a character or a
;;; small integer is compared by the `eq` primitive in place of a call to
;;; the library's `eql`: about a third of a clause's cost (#112).
(defun %case-compare (key k)
  (list (if (%eq-suffices-p k) 'eq 'eql) key (list 'quote k)))

;;; Is `eql` to OBJECT the same question as `eq` to it?
(defun %eq-suffices-p (object)
  (not (or (%bignump object) (floatp object) (%ratiop object))))

(defun %case-test (key keys)
  (if (null keys)
      nil
      (cons 'or (%case-tests key keys))))

(defun %case-tests (key keys)
  (if (null keys)
      nil
      (cons (%case-compare key (car keys)) (%case-tests key (cdr keys)))))

;;; `(destructuring-bind lambda-list expression body...)`: take EXPRESSION's
;;; value apart as a macro takes its form apart. The function is made by the
;;; compiler from the same rewrite `defmacro` uses, in its destructuring
;;; kind: a function of the one list, which `&whole` names (#76). A `nil` in
;;; the pattern is the empty pattern `()`, as in SBCL (#75).
(defmacro destructuring-bind (lambda-list expression &body body)
  `(funcall (%destructuring-lambda ,lambda-list ,@body) ,expression))

;;; A form that does not match its pattern is a `program-error` (CLHS
;;; 3.5.1.6), as SBCL signals; the prelude's, written for tier 0, signal a
;;; plain error (#75).
(defun %destructuring-too-short ()
  (error 'program-error "a macro call is missing arguments its parameter list asked for"))
(defun %destructuring-too-long ()
  (error 'program-error "a macro call has more arguments than its parameter list asked for"))

;;; ------------------------------------------------------------------------
;;; Iteration (CLHS 6)
;;;
;;; Every loop is a `block nil` around a `tagbody`, which is what `return`
;;; leaves and what a `go` inside the body may jump within: the body of each
;;; is an implicit `tagbody`, so it may hold tags of its own. The variables
;;; are bound once and assigned on each step, which CLHS allows; so a closure
;;; made in the body shares the one binding with every other, and after the
;;; loop sees what the loop left there — for `dolist`, nil.

(defmacro return (&optional value)
  `(return-from nil ,value))

;;; `(psetq var form ...)`: every form first, then every assignment. `psetf`
;;; on variables is exactly that, and is right for symbol macros too.
(defmacro psetq (&rest pairs)
  `(psetf ,@pairs))

;;; A body's leading `(declare ...)` forms, and the forms after them.
;;; The declarations go on the loop's bindings, where they cover the body and
;;; the result form; left in the `tagbody` they would be no-ops (`declare` is
;;; a macro expanding to nil), and a `special` one would be lost (#66).
(defun %body-declarations (body)
  (if (and (consp (car body)) (eq (car (car body)) 'declare))
      (cons (car body) (%body-declarations (cdr body)))
      nil))

(defun %body-forms (body)
  (if (and (consp (car body)) (eq (car (car body)) 'declare))
      (%body-forms (cdr body))
      body))

(defmacro dolist ((var list-form &optional result) &body body)
  (let ((tail (gensym "TAIL"))
        (top (gensym "TOP"))
        (end (gensym "END")))
    `(block nil
       (let ((,tail ,list-form)
             (,var nil))
         ,@(%body-declarations body)
         (tagbody
          ,top
            (if (endp ,tail) (go ,end))
            (setq ,var (car ,tail))
            ,@(%body-forms body)
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
         ,@(%body-declarations body)
         (tagbody
          ,top
            ;; The primitive `<`, not the library's n-ary `>=`: this test runs
            ;; on every iteration of every loop (ansi-test length.list.3).
            (if (< ,var ,count) nil (go ,end))
            ,@(%body-forms body)
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
        ,@(%body-declarations body)
        (tagbody
         ,top
           (if ,end-test (go ,end))
           ,@(%body-forms body)
           ,(if (eq stepper 'psetq)
                (%parallel-setq (%do-steps bindings))
                `(,stepper ,@(%do-steps bindings)))
           (go ,top)
         ,end)
        (progn ,@result)))))

(defun %do-inits (bindings)
  (if (null bindings)
      nil
      (let ((b (car bindings)))
        (cons (if (consp b) (list (car b) (car (cdr b))) (list b nil))
              (%do-inits (cdr bindings))))))

;;; Every variable of a `do` to its step, all steps computed first. `psetq`
;;; means that too, but by way of `psetf`, which must allow for any place:
;;; nested `let*` and `multiple-value-bind`, a closure and an `&optional`
;;; parse for each variable, on every trip round every loop — 39 steps an
;;; iteration where this is about a dozen (#112). A `do` variable is a plain
;;; variable, bound by the `do` itself, so temporaries and one `setq` do.
(defun %parallel-setq (pairs)
  (cond ((null pairs) nil)
        ((null (cdr (cdr pairs))) (cons 'setq pairs))
        (t (%parallel-setq-with pairs nil nil))))

;;; Written out with `cons`: `push` and `mapcar` are defined after the first
;;; `do` in this file is expanded.
(defun %parallel-setq-with (pairs bindings assignments)
  (if (null pairs)
      (list 'let (reverse bindings) (cons 'setq (reverse assignments)))
      (let ((temporary (gensym "STEP")))
        (%parallel-setq-with (cdr (cdr pairs))
                             (cons (list temporary (car (cdr pairs))) bindings)
                             (cons temporary (cons (car pairs) assignments))))))

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

;;; The three- and four-letter `c[ad]r`s this file itself uses, and their
;;; `setf` places, defined here so that the functions below can call them.
;;; `%define-cxr`, near the end of the file, defines every one again from
;;; its name; these are not dead, only early. `fourth` to `tenth` have their
;;; places from here alone.

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

;;; `endp` with the check CLHS asks for: the prelude's is `null`, and a
;;; non-list is a `type-error` (#75). A cons is asked about first: it is
;;; the common case, and the compiler calls `endp` on every list it walks,
;;; so the order decides whether a rebuild fits its step budget.
(defun endp (object)
  (cond ((consp object) nil)
        ((null object) t)
        (t (error 'type-error :datum object :expected-type 'list))))

;;; A count that is not a non-negative integer is a `type-error`, here and
;;; in `last`, `butlast` and `nbutlast`, which check through this (#75).
(defun %check-count (n)
  (if (or (not (integerp n)) (< n 0))
      (error 'type-error "wants a non-negative integer" n)))

;;; The walk stops at nil, whose every tail is nil: `(nthcdr 1000000 nil)`
;;; is nil at once, and `elt` past the end of a list finds out in as many
;;; steps as the list is long (ansi-test elt.2).
(defun nthcdr (n list)
  (%check-count n)
  (do ((i 0 (+ i 1))
       (tail list (cdr tail)))
      ((or (= i n) (null tail)) tail)))

;;; The prelude's `nth` counts down to zero and so never stops for a
;;; negative index: `(nth -1 list)` ran until the step budget did (#113).
;;; `nthcdr` checks its count.
(defun nth (n list) (car (nthcdr n list)))

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

;;; `last`, `butlast` and `nbutlast` count conses, not elements, so a dotted
;;; list works: `(last '(a b . c))` is `(b . c)`, and the atom at the end is
;;; the zeroth tail (#79). Until then they took the list's `length`, which
;;; refuses a dotted list.

;;; The last N conses: a lead pointer goes N conses ahead, then both walk
;;; until it falls off the end. N may be a bignum; the lead stops at the end
;;; long before that matters.
(defun last (list &optional (n 1))
  (%check-count n)
  (let ((lead list))
    (do ((i 0 (+ i 1)))
        ((or (not (< i n)) (atom lead)))
      (setq lead (cdr lead)))
    (do ((tail list (cdr tail))
         (lead lead (cdr lead)))
        ((atom lead) tail))))

;;; How many conses LIST has before its end, proper or dotted. Not a list
;;; at all is a `type-error`, as `length` made it before (#80).
(defun %cons-count (list)
  (if (not (listp list)) (error 'type-error "not a list" list))
  (do ((n 0 (+ n 1))
       (tail list (cdr tail)))
      ((atom tail) n)))

(defun butlast (list &optional (n 1))
  (%check-count n)
  (let ((keep (- (%cons-count list) n)))
    (do ((i 0 (+ i 1))
         (tail list (cdr tail))
         (out nil (cons (car tail) out)))
        ((>= i keep) (reverse out)))))

(defun nbutlast (list &optional (n 1))
  (%check-count n)
  (let ((keep (- (%cons-count list) n)))
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

;;; True or false, one value: a test may return several (#79).
(defun tree-equal (a b &key test test-not)
  (let ((same (%test test test-not)))
    (labels ((walk (a b)
               (cond ((and (consp a) (consp b))
                      (and (walk (car a) (car b)) (walk (cdr a) (cdr b))))
                     ((or (consp a) (consp b)) nil)
                     ((funcall same a b) t)
                     (t nil))))
      (walk a b))))

;;; Destructive append: each non-empty list's last cons is pointed at the
;;; next. The lists themselves are the result; nothing is copied.
;;; The last argument is only pointed at, never walked, so it may be
;;; circular or an atom: `(nconc x x)` makes X circular (#79). An atom
;;; before the last is skipped, as SBCL skips it.
(defun nconc (&rest lists)
  (let ((result nil)
        (last-cons nil))
    (do ((rest lists (cdr rest)))
        ((null rest) result)
      (let ((list (car rest)))
        (cond ((null (cdr rest))
               (if last-cons (rplacd last-cons list) (setq result list)))
              ((consp list)
               (if last-cons (rplacd last-cons list) (setq result list))
               (setq last-cons (last list))))))))

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
  ;; A list, possibly dotted; an atom alone is not one (SBCL, ansi-test).
  (unless (listp list)
    (error 'type-error :datum list :expected-type 'list))
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

;;; Over one list — nearly every call — there is nothing to keep in step:
;;; one walk, one call an element. The general walk above conses a list of
;;; tails and a list of heads and `apply`s for each element, 630 steps
;;; against some 40 (#112). By tail calls, which the machine runs as jumps.
(defun %mapcar-1 (function list out)
  (if (consp list)
      (%mapcar-1 function (cdr list) (cons (funcall function (car list)) out))
      (progn (endp list) (%prepend-reversed out nil))))

(defun %mapc-1 (function list)
  (if (consp list)
      (progn (funcall function (car list)) (%mapc-1 function (cdr list)))
      (endp list)))

(defun mapcar (function list &rest more)
  (if more
      (%map-lists function (cons list more) nil :collect)
      (%mapcar-1 function list nil)))
(defun mapc (function list &rest more)
  (if more
      (%map-lists function (cons list more) nil :discard)
      (progn (%mapc-1 function list) list)))
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

;;; The two walks first: the compiler calls `member` for every form it
;;; compiles, the next one included, so what the new `member` calls must
;;; exist before it does.
(defun %member-eq (item list)
  (cond ((consp list) (if (eq item (car list)) list (%member-eq item (cdr list))))
        (t (endp list) nil)))

(defun %assoc-eq (item alist)
  (cond ((consp alist)
         (if (and (consp (car alist)) (eq item (car (car alist))))
             (car alist)
             ;; A nil in an alist is skipped; anything else that is not a
             ;; cons is an error, which taking its `car` signals.
             (progn (if (car alist) (car (car alist)))
                    (%assoc-eq item (cdr alist)))))
        (t (endp alist) nil)))

;;; The prelude's `member` compares with `eq` and takes no keywords; this is
;;; CL's (decision 3). On the compiler's own two-argument calls it answers as
;;; the prelude's did, since `eql` is `eq`.
;;;
;;; With no keyword, and an item `eq` can decide — a symbol, a character, a
;;; small integer, which is nearly every call and every one of the
;;; compiler's — the search is a loop over `eq`, a primitive. Through
;;; `funcall` of the library's `eql` and a call of `%key` it was 100 steps
;;; an element (#112).
(defun member (item list &key key test test-not)
  (if (or key test test-not (not (%eq-suffices-p item)))
      (let ((same (%test test test-not)))
        (do ((tail list (cdr tail)))
            ((endp tail) nil)
          (if (funcall same item (%key key (car tail))) (return tail))))
      (%member-eq item list)))

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
  (if (or key test test-not (not (%eq-suffices-p item)))
      (let ((same (%test test test-not)))
        (dolist (pair alist nil)
          (if (and pair (funcall same item (%key key (car pair)))) (return pair))))
      (%assoc-eq item alist)))

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

;;; A function that says whether an element is in LIST-2, under KEY and the
;;; test. A long list with one of the four standard tests goes into a hash
;;; table first, as SBCL's set functions do: comparing every pair made
;;; `set-difference` of the 978 `COMMON-LISP` symbols run out of steps
;;; (#82). A short one is searched.
(defun %member-tester (list-2 key test test-not)
  (let ((table-test (if test-not nil (%hash-table-test-for test))))
    (if (and table-test (> (length list-2) 16))
        (let ((table (make-hash-table :test table-test)))
          (dolist (element list-2) (setf (gethash (%key key element) table) t))
          (lambda (element) (nth-value 1 (gethash (%key key element) table))))
        (let ((same (%test test test-not)))
          (lambda (element) (%member-by element list-2 key same))))))

(defun union (list-1 list-2 &key key test test-not)
  (let ((in-2 (%member-tester list-2 key test test-not))
        (out list-2))
    (dolist (element list-1 out)
      (if (not (funcall in-2 element)) (push element out)))))

(defun nunion (list-1 list-2 &key key test test-not)
  (union list-1 list-2 :key key :test test :test-not test-not))

(defun intersection (list-1 list-2 &key key test test-not)
  (let ((in-2 (%member-tester list-2 key test test-not))
        (out nil))
    (dolist (element list-1 (reverse out))
      (if (funcall in-2 element) (push element out)))))

(defun nintersection (list-1 list-2 &key key test test-not)
  (intersection list-1 list-2 :key key :test test :test-not test-not))

(defun set-difference (list-1 list-2 &key key test test-not)
  (let ((in-2 (%member-tester list-2 key test test-not))
        (out nil))
    (dolist (element list-1 (reverse out))
      (if (not (funcall in-2 element)) (push element out)))))

(defun nset-difference (list-1 list-2 &key key test test-not)
  (set-difference list-1 list-2 :key key :test test :test-not test-not))

(defun set-exclusive-or (list-1 list-2 &key key test test-not)
  (let ((same (%test test test-not)))
    ;; The test always gets an element of LIST-1 first and one of LIST-2
    ;; second (CLHS 17.2.1), so the second half swaps them back (#68).
    (append (set-difference list-1 list-2 :key key :test same)
            (set-difference list-2 list-1 :key key :test (lambda (b a) (funcall same a b))))))

(defun subsetp (list-1 list-2 &key key test test-not)
  (let ((in-2 (%member-tester list-2 key test test-not)))
    (dolist (element list-1 t)
      (if (not (funcall in-2 element)) (return nil)))))

;;; ------------------------------------------------------------------------
;;; Step U (#68): the rest of the list functions on ansi-test's list

;;; The destructive versions may be the plain ones: CLHS lets them modify
;;; their arguments and does not require it.
(defun nset-exclusive-or (list-1 list-2 &key key test test-not)
  (set-exclusive-or list-1 list-2 :key key :test test :test-not test-not))
(defun nsubst-if (new predicate tree &key key) (subst-if new predicate tree :key key))
(defun nsubst-if-not (new predicate tree &key key) (subst-if-not new predicate tree :key key))

;;; Every `c[ad]r` of three and four letters, with a `setf` place. A name is
;;; read left to right as the calls are written: `cadar` is the car of the
;;; cdr of the car. `%define-cxr` writes both from the name; the ones the
;;; prelude and `setf.lisp` already have are not touched.
(defun %cxr-op (name i) (if (eql (aref name i) #\A) 'car 'cdr))

;;; The form taking letters I to END of NAME around ARG.
(defun %cxr-reader (name i end arg)
  (if (>= i end) arg (list (%cxr-op name i) (%cxr-reader name (+ i 1) end arg))))

;;; Code that, run by the `setf` expander, builds that form around ARG.
(defun %cxr-builder (name i end arg)
  (if (>= i end)
      arg
      (list 'list (list 'quote (%cxr-op name i)) (%cxr-builder name (+ i 1) end arg))))

(defmacro %define-cxr (symbol)
  (let* ((name (symbol-name symbol))
         (end (- (length name) 1)))
    `(progn
       (defun ,symbol (x) ,(%cxr-reader name 1 end 'x))
       (defsetf ,symbol (x) (new)
         (list 'progn
               (list ',(if (eql (aref name 1) #\A) 'rplaca 'rplacd) ,(%cxr-builder name 2 end 'x) new)
               new)))))

(%define-cxr caaar) (%define-cxr caadr) (%define-cxr cadar) (%define-cxr cdaar)
(%define-cxr cdadr) (%define-cxr cddar) (%define-cxr cdddr)
(%define-cxr caaaar) (%define-cxr caaadr) (%define-cxr caadar) (%define-cxr caaddr)
(%define-cxr cadaar) (%define-cxr cadadr) (%define-cxr caddar) (%define-cxr cadddr)
(%define-cxr cdaaar) (%define-cxr cdaadr) (%define-cxr cdadar) (%define-cxr cdaddr)
(%define-cxr cddaar) (%define-cxr cddadr) (%define-cxr cdddar) (%define-cxr cddddr)

;;; `(prog (var...) body...)` — `let` and a `tagbody` inside `block nil`, the
;;; old way of writing a loop with `go`; `prog*` binds in order.
(defmacro prog (bindings &body body)
  `(block nil (let ,bindings ,@(%body-declarations body) (tagbody ,@(%body-forms body)))))

(defmacro prog* (bindings &body body)
  `(block nil (let* ,bindings ,@(%body-declarations body) (tagbody ,@(%body-forms body)))))

;;; `(multiple-value-setq (a b) form)` — assign the values; the first is
;;; the result. `setf` of `values` (#52) does the assigning.
(defmacro multiple-value-setq (variables form)
  (if variables
      `(values (setf (values ,@variables) ,form))
      `(values ,form)))
