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
