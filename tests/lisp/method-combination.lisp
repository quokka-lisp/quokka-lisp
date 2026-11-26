;;; Method combination (phase 3, CLOS sub-step 6, #124): the built-in
;;; operator combinations, define-method-combination in its short and long
;;; forms, call-method and make-method, invalid-method-error and
;;; method-combination-error. Tier 1: compiled and SBCL only.

(defvar *log* nil)
(defun take-log () (prog1 (reverse *log*) (setq *log* nil)))
(defun condition-kind (c)
  (cond ((typep c 'program-error) :program-error)
        ((typep c 'simple-error) :simple-error)
        (t :error)))

(class-name (defclass ka () ()))
(class-name (defclass kb (ka) ()))
(class-name (defclass kc (kb) ()))

;; + over every applicable method, most specific first.
(null (defgeneric k+ (x) (:method-combination +)))
(progn (defmethod k+ + ((x ka)) 1) (defmethod k+ + ((x kb)) 10) (defmethod k+ + ((x kc)) 100) :ok)
(list (k+ (make-instance 'ka)) (k+ (make-instance 'kb)) (k+ (make-instance 'kc)))

;; list, in both orders, and an :around method around it.
(null (defgeneric klist (x) (:method-combination list)))
(progn (defmethod klist list ((x ka)) :a) (defmethod klist list ((x kb)) :b) :ok)
(klist (make-instance 'kb))
(null (defgeneric klast (x) (:method-combination list :most-specific-last)))
(progn (defmethod klast list ((x ka)) :a) (defmethod klast list ((x kb)) :b) :ok)
(klast (make-instance 'kb))
(progn (defmethod klist :around ((x kb)) (cons :around (call-next-method))) :ok)
(klist (make-instance 'kb))
(klist (make-instance 'ka))

;; append, nconc, max, min.
(null (defgeneric kapp (x) (:method-combination append)))
(progn (defmethod kapp append ((x ka)) (list 1)) (defmethod kapp append ((x kb)) (list 2 3)) :ok)
(kapp (make-instance 'kb))
(null (defgeneric knc (x) (:method-combination nconc)))
(progn (defmethod knc nconc ((x ka)) (list 1)) (defmethod knc nconc ((x kb)) (list 2)) :ok)
(knc (make-instance 'kb))
(null (defgeneric kmax (x) (:method-combination max)))
(progn (defmethod kmax max ((x ka)) 3) (defmethod kmax max ((x kb)) 2) :ok)
(kmax (make-instance 'kb))
(null (defgeneric kmin (x) (:method-combination min :most-specific-last)))
(progn (defmethod kmin min ((x ka)) 3) (defmethod kmin min ((x kb)) 2) :ok)
(kmin (make-instance 'kb))

;; One method alone: its values pass through, except under list.
(null (defgeneric kor (x) (:method-combination or)))
(progn (defmethod kor or ((x ka)) (values nil 1)) (defmethod kor or ((x kb)) (values 7 8)) :ok)
(multiple-value-list (kor (make-instance 'ka)))
(multiple-value-list (kor (make-instance 'kb)))
(null (defgeneric kprogn (x) (:method-combination progn)))
(progn (defmethod kprogn progn ((x ka)) (values 1 2)) :ok)
(multiple-value-list (kprogn (make-instance 'ka)))
(null (defgeneric kplus1 (x) (:method-combination +)))
(progn (defmethod kplus1 + ((x ka)) (values 3 4)) :ok)
(multiple-value-list (kplus1 (make-instance 'ka)))
(null (defgeneric klist1 (x) (:method-combination list)))
(progn (defmethod klist1 list ((x ka)) (values 3 4)) :ok)
(multiple-value-list (klist1 (make-instance 'ka)))

;; and, or and progn run their methods in order and stop as the operators do.
(null (defgeneric kand (x) (:method-combination and)))
(progn (defmethod kand and ((x ka)) (push :a *log*) (values 1 2))
       (defmethod kand and ((x kb)) (push :b *log*) t)
       :ok)
(list (multiple-value-list (kand (make-instance 'kb))) (take-log))
(progn (defmethod kand and ((x kc)) (push :c *log*) nil) :ok)
(list (kand (make-instance 'kc)) (take-log))
(progn (defmethod kor or ((x kc)) (push :c *log*) nil) :ok)
(list (multiple-value-list (kor (make-instance 'kc))) (take-log))
(null (defgeneric kseq (x) (:method-combination progn :most-specific-last)))
(progn (defmethod kseq progn ((x ka)) (push :a *log*) 1) (defmethod kseq progn ((x kb)) (push :b *log*) 2) :ok)
(list (kseq (make-instance 'kb)) (take-log))

;; A primary method has no next method; an :around method does.
(null (defgeneric knext (x) (:method-combination +)))
(progn (defmethod knext + ((x ka)) 1) (defmethod knext + ((x kb)) (if (next-method-p) 100 0)) :ok)
(knext (make-instance 'kb))
(progn (defmethod knext :around ((x kb)) (list (next-method-p) (call-next-method))) :ok)
(knext (make-instance 'kb))

;; Methods the combination does not accept, and no primary method.
(progn (defmethod kprogn ((x kb)) 0) :ok)
(handler-case (kprogn (make-instance 'kb)) (error (c) (condition-kind c)))
(multiple-value-list (kprogn (make-instance 'ka)))
(null (defgeneric kbefore (x) (:method-combination progn)))
(progn (defmethod kbefore progn ((x ka)) 1) (defmethod kbefore :before ((x ka)) 0) :ok)
(handler-case (kbefore (make-instance 'ka)) (error (c) (condition-kind c)))
(progn (defmethod knext + :extra ((x ka)) 1) :ok)
(handler-case (knext (make-instance 'kb)) (error (c) (condition-kind c)))
(null (defgeneric knone (x) (:method-combination +)))
(progn (defmethod knone :around ((x ka)) 1) :ok)
(handler-case (knone (make-instance 'ka)) (error (c) (typep c 'program-error)))
(handler-case (knone 5) (error (c) (typep c 'error)))

;; The two errors a combination signals.
(handler-case (method-combination-error "bad ~A" 1) (error (c) (condition-kind c)))
(handler-case (invalid-method-error (find-method #'kprogn '() '(kb)) "bad ~A" 1)
  (error (c) (condition-kind c)))

;; Keyword arguments are checked as under standard combination (CLHS 7.6.5).
(null (defgeneric kkey (x &key) (:method-combination +)))
(progn (defmethod kkey + ((x ka) &key (k 1)) k) (defmethod kkey + ((x kb) &key (j 2)) j) :ok)
(list (kkey (make-instance 'kb)) (kkey (make-instance 'kb) :k 10 :j 20))
(handler-case (kkey (make-instance 'ka) :j 5) (error (c) (condition-kind c)))
(kkey (make-instance 'ka) :j 5 :allow-other-keys t)

;; A generic function whose combination changes after a call: the methods
;; it had are no longer valid, and the call says so.
(null (defgeneric kchange (x) (:method-combination +)))
(progn (defmethod kchange + ((x ka)) 1) (defmethod kchange + ((x kb)) 2) :ok)
(kchange (make-instance 'kb))
(null (defgeneric kchange (x) (:method-combination list)))
(handler-case (kchange (make-instance 'kb)) (error (c) (condition-kind c)))
(progn (defmethod kchange list ((x ka)) 1) (defmethod kchange list ((x kb)) 2)
       (remove-method #'kchange (find-method #'kchange '(+) '(ka)))
       (remove-method #'kchange (find-method #'kchange '(+) '(kb)))
       :ok)
(kchange (make-instance 'kb))

;; No primary method is checked before methods the combination does not
;; accept.
(null (defgeneric kmixed (x) (:method-combination +)))
(progn (defmethod kmixed + ((x kb)) 1) (defmethod kmixed list ((x ka)) 2) :ok)
(handler-case (kmixed (make-instance 'ka)) (error (c) (condition-kind c)))
(handler-case (kmixed (make-instance 'kb)) (error (c) (condition-kind c)))
(handler-case (kmixed 5) (error (c) (condition-kind c)))

;; An unknown combination is an error when the generic function is defined.
(handler-case (defgeneric kunknown (x) (:method-combination no-such-combination))
  (error (c) (condition-kind c)))

;; Options a combination does not take are an error when the generic
;; function is defined.
(handler-case (defgeneric kbadopt (x) (:method-combination + :bogus)) (error (c) (condition-kind c)))
(handler-case (defgeneric kbadstd (x) (:method-combination standard :most-specific-last))
  (error (c) (condition-kind c)))

;; A defgeneric without the option keeps the combination the function has.
(null (defgeneric kkeep (x) (:method-combination +)))
(progn (defmethod kkeep + ((x ka)) 1) (defmethod kkeep + ((x kb)) 2) :ok)
(null (defgeneric kkeep (x)))
(kkeep (make-instance 'kb))

;;; define-method-combination, short form.

(define-method-combination my-list :operator list :identity-with-one-argument nil)
(null (defgeneric smylist (x) (:method-combination my-list)))
(progn (defmethod smylist my-list ((x ka)) :a) (defmethod smylist my-list ((x kb)) :b) :ok)
(list (smylist (make-instance 'kb)) (smylist (make-instance 'ka)))
(define-method-combination my-plus :operator + :identity-with-one-argument t)
(null (defgeneric smyplus (x) (:method-combination my-plus :most-specific-last)))
(progn (defmethod smyplus my-plus ((x ka)) (values 5 6)) :ok)
(multiple-value-list (smyplus (make-instance 'ka)))
;; With no :operator the operator is the name; identity defaults to false.
(defun join (&rest lists) (apply #'append lists))
(define-method-combination join)
(null (defgeneric sjoin (x) (:method-combination join)))
(progn (defmethod sjoin join ((x ka)) (list 1)) (defmethod sjoin join ((x kb)) (list 2 3)) :ok)
(sjoin (make-instance 'kb))
(sjoin (make-instance 'ka))
;; An operator that is a macro.
(defmacro tagged (&rest forms) (list* 'list :tagged forms))
(define-method-combination tagged-c :operator tagged)
(null (defgeneric stagged (x) (:method-combination tagged-c)))
(progn (defmethod stagged tagged-c ((x ka)) 1) (defmethod stagged tagged-c ((x kb)) 2) :ok)
(list (stagged (make-instance 'kb)) (stagged (make-instance 'ka)))
;; Defined again after a call: the next call uses the new definition.
(define-method-combination tagged-c :operator list)
(stagged (make-instance 'kb))

;;; define-method-combination, long form.

;; CLHS's own example: standard method combination written out.
(define-method-combination standard2 ()
    ((around (:around))
     (before (:before))
     (primary () :required t)
     (after (:after)))
  (flet ((call-methods (methods)
           (mapcar #'(lambda (method) `(call-method ,method)) methods)))
    (let ((form (if (or before after (rest primary))
                    `(multiple-value-prog1
                         (progn ,@(call-methods before)
                                (call-method ,(first primary) ,(rest primary)))
                       ,@(call-methods (reverse after)))
                    `(call-method ,(first primary)))))
      (if around
          `(call-method ,(first around) (,@(rest around) (make-method ,form)))
          form))))
(null (defgeneric lstd (x) (:method-combination standard2)))
(progn (defmethod lstd ((x ka)) (push :a *log*) :a)
       (defmethod lstd ((x kb)) (push :b *log*) (list :b (call-next-method)))
       (defmethod lstd :before ((x ka)) (push :before-a *log*))
       (defmethod lstd :after ((x kb)) (push :after-b *log*))
       (defmethod lstd :around ((x kc)) (push :around *log*) (list :around (call-next-method)))
       :ok)
(list (lstd (make-instance 'kc)) (take-log))
(list (lstd (make-instance 'ka)) (take-log))
;; A required group with no method.
(null (defgeneric lreq (x) (:method-combination standard2)))
(progn (defmethod lreq :before ((x ka)) 1) :ok)
(handler-case (lreq (make-instance 'ka)) (error (c) (condition-kind c)))
;; A method no group takes.
(progn (defmethod lreq :bogus ((x ka)) 1) (defmethod lreq ((x ka)) 2) :ok)
(handler-case (lreq (make-instance 'ka)) (error (c) (condition-kind c)))

;; :order from an option, and options checked when the function is called.
(define-method-combination ordered (&optional (order :most-specific-first))
  ((methods * :order order))
  `(list ,@(mapcar (lambda (m) `(call-method ,m)) methods)))
(null (defgeneric lord (x) (:method-combination ordered :most-specific-last)))
(progn (defmethod lord :x ((x ka)) 1) (defmethod lord :y ((x kb)) 2) :ok)
(lord (make-instance 'kb))
(null (defgeneric lord2 (x) (:method-combination ordered)))
(progn (defmethod lord2 ((x ka)) 1) (defmethod lord2 ((x kb)) 2) :ok)
(lord2 (make-instance 'kb))
(null (defgeneric lord3 (x) (:method-combination ordered :sideways)))
(progn (defmethod lord3 ((x ka)) 1) :ok)
(handler-case (lord3 (make-instance 'ka)) (error (c) (condition-kind c)))
(define-method-combination keyed (&key (order :most-specific-first))
  ((methods * :order order))
  `(list ,@(mapcar (lambda (m) `(call-method ,m)) methods)))
(null (defgeneric lkey (x) (:method-combination keyed :order :most-specific-last)))
(progn (defmethod lkey :p ((x ka)) 1) (defmethod lkey :q ((x kb)) 2) :ok)
(lkey (make-instance 'kb))
(null (defgeneric lkey2 (x) (:method-combination keyed :colour :red)))
(progn (defmethod lkey2 :p ((x ka)) 1) :ok)
(handler-case (lkey2 (make-instance 'ka)) (error (c) (condition-kind c)))

;; Qualifier patterns and predicates.
(defun even-first-p (qualifiers)
  (and (consp qualifiers) (integerp (car qualifiers)) (evenp (car qualifiers))))
(define-method-combination by-predicate ()
    ((evens even-first-p) (others * :description "the rest"))
  `(list ',(length evens) ',(length others)))
(null (defgeneric lpred (x) (:method-combination by-predicate)))
(progn (defmethod lpred 2 ((x ka)) 0) (defmethod lpred 3 ((x kb)) 0) (defmethod lpred 4 ((x kb)) 0)
       (defmethod lpred ((x kc)) 0) :ok)
(lpred (make-instance 'kc))
(define-method-combination patterns () ((one (:x *)) (two (:y . *)) (none ()))
  `(list ',(length one) ',(length two) ',(length none)))
(null (defgeneric lpat (x) (:method-combination patterns)))
(progn (defmethod lpat :x 1 ((x ka)) 0) (defmethod lpat :y ((x ka)) 0) (defmethod lpat :y 1 2 ((x kb)) 0)
       (defmethod lpat ((x ka)) 0) :ok)
(lpat (make-instance 'kb))
(progn (defmethod lpat :x ((x kb)) 0) :ok)
(handler-case (lpat (make-instance 'kb)) (error (c) (condition-kind c)))

;; Two methods of a group with the same specializers, unless the only
;; group takes every method.
(define-method-combination two-groups () ((ms (:p) (:q)) (other *))
  `(list ,@(mapcar (lambda (m) `(call-method ,m)) ms)))
(null (defgeneric ldup (x) (:method-combination two-groups)))
(progn (defmethod ldup :p ((x ka)) 1) (defmethod ldup :q ((x ka)) 2) :ok)
(handler-case (ldup (make-instance 'ka)) (error (c) (condition-kind c)))
(define-method-combination every-method () ((ms *))
  `(list ,@(mapcar (lambda (m) `(call-method ,m)) ms)))
(null (defgeneric levery (x) (:method-combination every-method)))
(progn (defmethod levery :p ((x ka)) 1) (defmethod levery :q ((x kb)) 2) :ok)
(levery (make-instance 'kb))

;; :arguments and :generic-function.
(define-method-combination with-arguments () ((ms *))
  (:arguments &whole whole first &optional (second 5) &rest others)
  (:generic-function gf)
  `(list ,whole ,first ,second ,others ',(typep gf 'generic-function) (call-method ,(first ms))))
(null (defgeneric larg (p &optional q &rest more) (:method-combination with-arguments)))
(progn (defmethod larg ((p integer) &optional q &rest more) (list q more)) :ok)
(larg 1)
(larg 1 2 3 4)
(define-method-combination too-many-arguments () ((ms *)) (:arguments x y z)
  `(list ,x ,y ,z (call-method ,(first ms))))
(null (defgeneric larg2 (p q) (:method-combination too-many-arguments)))
(progn (defmethod larg2 ((p integer) q) q) :ok)
(larg2 1 7)

;; call-method with a list of next methods, make-method among them.
(define-method-combination chained () ((ms *))
  `(call-method ,(first ms)
                (,@(rest ms) (make-method (list :made (call-method ,(car (last ms))))))))
(null (defgeneric lchain (x) (:method-combination chained)))
(progn (defmethod lchain :one ((x kc)) (list :c (next-method-p) (call-next-method)))
       (defmethod lchain :two ((x kb)) (list :b (next-method-p) (call-next-method)))
       (defmethod lchain :three ((x ka)) (list :a (next-method-p) (if (next-method-p) (call-next-method) :end)))
       :ok)
(lchain (make-instance 'kc))
(lchain (make-instance 'kb))
(null (defgeneric lmade (x) (:method-combination chained)))
;; The arguments call-next-method passes reach the make-method form.
(progn (defmethod lmade :one ((x integer))
         (if (next-method-p) (list :outer x (call-next-method 7)) (list :inner x)))
       :ok)
(lmade 1)

;; The errors a body signals reach the caller, and nothing is kept.
(define-method-combination picky () ((ms *))
  (if (cdr ms)
      (method-combination-error "too many methods: ~D" (length ms))
      `(call-method ,(first ms))))
(null (defgeneric lpicky (x) (:method-combination picky)))
(progn (defmethod lpicky ((x ka)) 1) (defmethod lpicky :z ((x kb)) 2) :ok)
(handler-case (lpicky (make-instance 'kb)) (error (c) (condition-kind c)))
(lpicky (make-instance 'ka))
(progn (remove-method #'lpicky (find-method #'lpicky '(:z) '(kb))) :ok)
(lpicky (make-instance 'kb))
(define-method-combination failing () ((ms *))
  (if (cdr ms) (error "the body fails") '(1 2 . 3)))
(null (defgeneric lfail (x) (:method-combination failing)))
(progn (defmethod lfail ((x ka)) 1) (defmethod lfail :z ((x kb)) 2) :ok)
(handler-case (lfail (make-instance 'kb)) (error (c) (condition-kind c)))
(handler-case (lfail (make-instance 'ka)) (error () :error))
(define-method-combination failing () ((ms *)) `(call-method ,(car (last ms))))
(list (lfail (make-instance 'kb)) (lfail (make-instance 'ka)))

;; Keyword arguments under a long form.
(null (defgeneric lkeys (x &key) (:method-combination every-method)))
(progn (defmethod lkeys ((x ka) &key (k 1)) k) (defmethod lkeys :b ((x kb) &key (j 2)) j) :ok)
(lkeys (make-instance 'kb) :k 10)
(handler-case (lkeys (make-instance 'ka) :j 5) (error (c) (condition-kind c)))

;; call-method is a global macro too, an error outside an effective method
;; form; make-method is not.
(not (null (macro-function 'call-method)))
(fboundp 'make-method)
(handler-case (eval '(call-method nil)) (error (c) (condition-kind c)))

;; :arguments with &aux, supplied-p variables, and defaults that read an
;; earlier variable; the body sees each variable's own name.
(define-method-combination aux-arguments () ((ms *)) (:arguments x &aux (y 3))
  `(list ,x ,y (call-method ,(first ms))))
(null (defgeneric laux (x) (:method-combination aux-arguments)))
(progn (defmethod laux ((x integer)) :m) :ok)
(laux 1)
(define-method-combination supplied-arguments () ((ms *))
  (:arguments x &optional (y x y-p) &key (k nil k-p))
  `(list ,x ,y ,y-p ,k ,k-p ',y))
(null (defgeneric lsupplied (x &optional y &key k) (:method-combination supplied-arguments)))
(progn (defmethod lsupplied ((x integer) &optional y &key k) (list y k)) :ok)
(lsupplied 1)
(lsupplied 1 2 :k 5)

;; Under standard combination too, no primary method is reported before a
;; qualifier the combination does not accept.
(null (defgeneric lstdbad (x)))
(progn (defmethod lstdbad :bogus ((x integer)) 1) (defmethod lstdbad :before ((x integer)) 2) :ok)
(handler-case (lstdbad 1) (error (c) (condition-kind c)))
(progn (defmethod lstdbad ((x integer)) 3) :ok)
(handler-case (lstdbad 1) (error (c) (condition-kind c)))

;; :required is not evaluated: any form there counts as true.
(define-method-combination required-form (&optional (req t)) ((pri () :required req) (other *))
  `(list ',(length pri) ',(length other)))
(null (defgeneric lreqform (x) (:method-combination required-form nil)))
(progn (defmethod lreqform :q ((x integer)) 0) :ok)
(handler-case (lreqform 1) (error (c) (condition-kind c)))

;; Qualifier patterns compare qualifiers with eql, as SBCL does: a string
;; matches no pattern but *, a number, a character or a keyword its own.
(define-method-combination eql-patterns ()
    ((strings ("s")) (floats (1.5)) (characters (#\x)) (integers (2)) (keywords (:k)) (other *))
  `(list ',(length strings) ',(length floats) ',(length characters) ',(length integers)
         ',(length keywords) ',(length other)))
(null (defgeneric leqlpat (x) (:method-combination eql-patterns)))
(progn (defmethod leqlpat "s" ((x integer)) 0) (defmethod leqlpat 1.5 ((x number)) 0)
       (defmethod leqlpat #\x ((x real)) 0) (defmethod leqlpat 2 ((x rational)) 0)
       (defmethod leqlpat :k ((x t)) 0) :ok)
(leqlpat 1)

;; Something other than a method in a next-method list is an error only
;; when it is called.
(define-method-combination odd-next () ((ms *)) `(call-method ,(first ms) (5)))
(null (defgeneric loddnext (x) (:method-combination odd-next)))
(progn (defmethod loddnext ((x integer)) 1) :ok)
(loddnext 1)
(progn (defmethod loddnext ((x integer)) (next-method-p)) :ok)
(loddnext 1)
(progn (defmethod loddnext ((x integer)) (call-next-method)) :ok)
(handler-case (loddnext 1) (error (c) (list (condition-kind c) (typep c 'type-error))))
