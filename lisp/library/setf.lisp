;;;; setf expanders (#52, step H of #39).
;;;;
;;;; The prelude's `setf` knows a fixed list of places and a table from a
;;;; reader's name to a writer's name. That is enough to *store* into a place
;;;; and not enough to *update* one: `(incf (aref v (next-index)))` has to
;;;; read the element and write it back while calling `next-index` once.
;;;;
;;;; Common Lisp's answer is the **setf expansion** (CLHS 5.1.1.2). For any
;;;; place, `get-setf-expansion` returns five values:
;;;;
;;;;   vars    temporaries                          (#:v #:i)
;;;;   vals    the subforms they are bound to       (v (next-index))
;;;;   stores  variables that will hold new values  (#:new)
;;;;   writer  stores them, and returns them        (%set-aref #:v #:i #:new)
;;;;   reader  reads the place                      (aref #:v #:i)
;;;;
;;;; A macro binds the temporaries once, reads with the reader, binds the
;;;; store, and runs the writer. Every subform is evaluated exactly once, left
;;;; to right, whatever the macro does with the place.
;;;;
;;;; Decided in #52 (Carsten):
;;;;
;;;; 1. **A place the prelude knows expands exactly as it did.** This file
;;;;    redefines `setf`, and the warm rebuild compiles the compiler with the
;;;;    `setf` of the running world — this one. If `(setf (gethash k h) v)`
;;;;    came out differently, the compiler would be compiled from different
;;;;    code and the fixpoint (#50) would fail. So `setf` asks the prelude's
;;;;    `%setf-known-place` first, and only a place it does not know goes
;;;;    through the five values. `get-setf-expansion` builds its five values
;;;;    from the *same* function, so one place cannot mean one thing to
;;;;    `setf` and another to `incf`. SBCL does the same: `(setf (car x) v)`
;;;;    is `(sb-kernel:%rplaca x v)`, with no temporaries, because a plain
;;;;    store does not need them.
;;;; 2. The prelude's `(setf (car p) v)` evaluated `v` before `p`; fixed there.
;;;; 3. Places beyond the prelude's: the ones whose reader exists, plus
;;;;    `getf`, the case that shows why five values and not a writer's name.
;;;;
;;;; Every macro here takes `&environment` and hands it to
;;;; `get-setf-expansion`, so a symbol macro or a `macrolet` macro is seen as
;;;; a place (#53). Until step I there was no environment to hand on.
;;;;
;;;; There is no `mapcar` or `dolist` yet (sequences are step J), so the
;;;; walking below is done with `labels`.

;;; ------------------------------------------------------------------------
;;; The expander table
;;;
;;; Two tables, one entry per name. The prelude's `%setf-setters` (reader →
;;; writer) is the short form of `defsetf`, and `defstruct` fills it. This one
;;; holds the general expanders: a function from the place's argument forms
;;; to five values. Defining a name in one removes it from the other.

(defvar %setf-expanders (make-hash-table))

(defun %define-setf-expander (name expander)
  (setf (gethash name %setf-setters) nil)
  (setf (gethash name %setf-expanders) expander)
  name)

;;; `defsetf`'s short form is the prelude's table under its standard name.
(defun %define-setf-short (name writer)
  (setf (gethash name %setf-expanders) nil)
  (%define-setf-writer name writer)
  name)

;;; ------------------------------------------------------------------------
;;; get-setf-expansion

(defun get-setf-expansion (place &optional environment)
  (cond
    ;; A symbol macro is the place it stands for (#53). A variable needs no
    ;; temporaries: read it, `setq` it.
    ((symbolp place)
     (multiple-value-bind (expansion expanded) (macroexpand-1 place environment)
       (if expanded
           (get-setf-expansion expansion environment)
           (let ((store (gensym "NEW")))
             (values nil nil (list store) (list 'setq place store) place)))))
    ((not (consp place)) (%setf-unknown-place place))
    ;; A place the prelude knows: the writer is what the prelude's `setf`
    ;; would write, with temporaries in place of the subforms.
    ((%setf-known-place place nil)
     (multiple-value-bind (vars vals args) (%temporaries (cdr place))
       (let ((store (gensym "NEW")))
         (values vars vals (list store)
                 (%setf-known-place (cons (car place) args) store)
                 (cons (car place) args)))))
    ;; A general expander, from `define-setf-expander` or long `defsetf`.
    ((gethash (car place) %setf-expanders)
     (apply (gethash (car place) %setf-expanders) (cdr place)))
    ;; A macro form is a place if what it expands to is (CLHS 5.1.2.7).
    (t (multiple-value-bind (expansion expanded) (macroexpand-1 place environment)
         (if expanded
             (get-setf-expansion expansion environment)
             (%setf-unknown-place place))))))

;;; A temporary for each form, except a constant, which stands for itself:
;;; nothing to evaluate twice, and a keyword argument to a long `defsetf`
;;; must still look like a keyword. Three values: the temporaries, the forms
;;; they are bound to, and the argument list to use in the writer and reader.
(defun %temporaries (forms)
  (labels ((walk (rest vars vals args)
             (cond ((null rest) (values (reverse vars) (reverse vals) (reverse args)))
                   ((%constant-form-p (car rest))
                    (walk (cdr rest) vars vals (cons (car rest) args)))
                   (t (let ((var (gensym)))
                        (walk (cdr rest) (cons var vars) (cons (car rest) vals)
                              (cons var args)))))))
    (walk forms nil nil nil)))

(defun %constant-form-p (form)
  (or (keywordp form) (integerp form) (stringp form) (characterp form)
      (eq form t) (null form)
      (and (consp form) (eq (car form) 'quote))))

;;; `((var val) ...)` for `let*`.
(defun %bindings (vars vals)
  (if (null vars)
      nil
      (cons (list (car vars) (car vals)) (%bindings (cdr vars) (cdr vals)))))

;;; The macros that read and write a place use one store; only `setf` itself
;;; can fill several (a `values` place).
(defun %one-store (stores place)
  (if (and (consp stores) (null (cdr stores)))
      (car stores)
      (error "this place takes more than one value here" place)))

;;; ------------------------------------------------------------------------
;;; setf

;;; The prelude's `setf`, pair for pair, except that a place the prelude does
;;; not know goes through its expansion instead of being an error. The shape
;;; — the `progn`, the odd-number error — is copied exactly, because a known
;;; place must expand as before (decision 1).
(defmacro setf (&rest pairs &environment environment)
  (labels ((expand (rest)
             (cond ((null rest) nil)
                   ((null (cdr rest))
                    (error "SETF wants place and value in pairs" (car rest)))
                   (t (cons (%setf-pair (car rest) (car (cdr rest)) environment)
                            (expand (cdr (cdr rest))))))))
    (let ((forms (expand pairs)))
      (cond ((null forms) nil)
            ((null (cdr forms)) (car forms))
            (t (cons 'progn forms))))))

(defun %setf-pair (place value environment)
  (or (%setf-known-place place value)
      (multiple-value-bind (vars vals stores writer) (get-setf-expansion place environment)
        (if (and (consp stores) (null (cdr stores)))
            `(let* (,@(%bindings vars vals) (,(car stores) ,value)) ,writer)
            `(let* ,(%bindings vars vals)
               (multiple-value-bind ,stores ,value ,writer))))))

;;; ------------------------------------------------------------------------
;;; Defining places

;;; `(defsetf name writer)` — the short form: `(setf (name args...) v)` is
;;; `(writer args... v)`, and the writer returns v.
;;;
;;; `(defsetf name lambda-list (store...) body...)` — the long form: BODY
;;; computes the writer, with the parameters bound to the *temporaries* that
;;; hold the place's subforms, and the stores to the store variables. The
;;; function below takes the stores first, then the place's arguments, so an
;;; ordinary lambda list does the binding.
(defmacro defsetf (name &rest rest)
  (if (and (consp rest) (symbolp (car rest)) (car rest))
      `(%define-setf-short ',name ',(car rest))
      (let ((lambda-list (car rest))
            (stores (car (cdr rest)))
            (body (cdr (cdr rest)))
            (arguments (gensym "ARGUMENTS")))
        (multiple-value-bind (plain environment) (%without-environment lambda-list)
          `(%define-setf-expander
            ',name
            (lambda (&rest ,arguments)
              (%long-defsetf-expansion
               ',name ,arguments ,(length stores)
               (lambda (,@stores ,@plain)
                 (let ((,environment (%current-environment))) ,environment ,@body)))))))))

(defun %long-defsetf-expansion (name forms count writer)
  (multiple-value-bind (vars vals args) (%temporaries forms)
    (let ((stores (%gensyms count)))
      (values vars vals stores
              (apply writer (append stores args))
              (cons name args)))))

(defun %gensyms (count)
  (if (= count 0) nil (cons (gensym "NEW") (%gensyms (- count 1)))))

;;; `(define-setf-expander name lambda-list body...)` — BODY returns the five
;;; values itself. Its lambda list is a macro's, destructuring and
;;; `&environment` included, so the expander is made the way `defmacro` makes
;;; one, under a name nobody can write (a fresh uninterned symbol), and taken
;;; back out as a function. It is called while a `setf` is being expanded,
;;; so `&environment` is that `setf`'s.
(defmacro define-setf-expander (name lambda-list &body body)
  (let ((hidden (gensym "SETF-EXPANDER")))
    `(progn
       (defmacro ,hidden ,lambda-list ,@body)
       (%define-setf-expander ',name (%macro-function ',hidden)))))

;;; A lambda list without its `&environment VAR`, and VAR (a gensym if there
;;; was none), for long `defsetf`, whose lambda list is an ordinary one.
(defun %without-environment (lambda-list)
  (labels ((walk (rest kept)
             (cond ((not (consp rest)) (values (append (reverse kept) rest) (gensym "ENV")))
                   ((eq (car rest) '&environment)
                    (values (append (reverse kept) (cdr (cdr rest))) (car (cdr rest))))
                   (t (walk (cdr rest) (cons (car rest) kept))))))
    (walk lambda-list nil)))

;;; ------------------------------------------------------------------------
;;; Macros that read and write a place

;;; `(define-modify-macro name lambda-list function)` defines a macro
;;; `(name place args...)` that stores `(function old-value args...)`. The
;;; lambda list has only `&optional` and `&rest`, per CLHS.
(defmacro define-modify-macro (name lambda-list function &optional documentation)
  documentation
  (let ((place (gensym "PLACE"))
        (environment (gensym "ENV")))
    `(defmacro ,name (,place ,@lambda-list &environment ,environment)
       (%modify-place ,place ',function ,(%modify-arguments lambda-list) ,environment))))

;;; The form that lists a modify macro's arguments, rest list included.
(defun %modify-arguments (lambda-list)
  (labels ((walk (rest required)
             (cond ((null rest) (cons 'list (reverse required)))
                   ((eq (car rest) '&optional) (walk (cdr rest) required))
                   ((eq (car rest) '&rest)
                    (list 'append (cons 'list (reverse required)) (car (cdr rest))))
                   ((consp (car rest)) (walk (cdr rest) (cons (car (car rest)) required)))
                   (t (walk (cdr rest) (cons (car rest) required))))))
    (walk lambda-list nil)))

(defun %modify-place (place function arguments environment)
  (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
    `(let* (,@(%bindings vars vals)
            (,(%one-store stores place) (,function ,reader ,@arguments)))
       ,writer)))

(define-modify-macro incf (&optional (delta 1)) +)
(define-modify-macro decf (&optional (delta 1)) -)

;;; `push` evaluates ITEM *before* the place's subforms — the one exception
;;; to left-to-right that CLHS 5.1.1.1 needs, since ITEM is written first.
(defmacro push (item place &environment environment)
  (let ((value (gensym "ITEM")))
    (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
      `(let* ((,value ,item)
              ,@(%bindings vars vals)
              (,(%one-store stores place) (cons ,value ,reader)))
         ,writer))))

;;; `pushnew` compares with `eq` by default where CL says `eql`: Quokka has no
;;; `eql` until bignums, and on everything it has today the two agree.
(defmacro pushnew (item place &rest keys &environment environment)
  (let ((value (gensym "ITEM")))
    (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
      `(let* ((,value ,item)
              ,@(%bindings vars vals)
              (,(%one-store stores place) (%adjoin ,value ,reader ,@keys)))
         ,writer))))

;;; `adjoin`'s work, for `pushnew`, until step J gives the list functions a
;;; home of their own.
(defun %adjoin (item list &key key test test-not)
  (let ((probe (if key (funcall key item) item)))
    (labels ((same (element)
               (let ((x (if key (funcall key element) element)))
                 (cond (test (funcall test probe x))
                       (test-not (not (funcall test-not probe x)))
                       (t (eq probe x)))))
             (present (rest)
               (cond ((null rest) nil)
                     ((same (car rest)) t)
                     (t (present (cdr rest))))))
      (if (present list) list (cons item list)))))

(defmacro pop (place &environment environment)
  (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
    (let ((list (gensym "LIST")))
      `(let* (,@(%bindings vars vals)
              (,list ,reader)
              (,(%one-store stores place) (cdr ,list)))
         ,writer
         (car ,list)))))

;;; `(shiftf p1 ... pn new)`: each place takes the next one's old value, the
;;; last takes NEW, and the result is p1's old value. All subforms first, in
;;; order; then all reads; then all writes.
(defmacro shiftf (&rest places-and-value &environment environment)
  (let ((places (reverse (cdr (reverse places-and-value))))
        (new (car (reverse places-and-value)))
        (old (gensym "OLD")))
    (multiple-value-bind (bindings stores writers readers) (%expand-places places environment)
      `(let* (,@bindings
              (,old ,(car readers))
              ,@(%bindings stores (append (cdr readers) (list new))))
         ,@writers
         ,old))))

;;; `(rotatef p1 ... pn)`: each place takes the next one's value, the last
;;; takes the first's. Nil.
(defmacro rotatef (&rest places &environment environment)
  (if (null places)
      nil
      (multiple-value-bind (bindings stores writers readers) (%expand-places places environment)
        `(let* (,@bindings
                ,@(%bindings stores (append (cdr readers) (list (car readers)))))
           ,@writers
           nil))))

;;; `(psetf p1 v1 p2 v2 ...)`: like `setf`, but every value is computed
;;; before any place is written. Nil. Each value is bound right after its
;;; own place's subforms, which keeps the order of evaluation the source's.
(defmacro psetf (&rest pairs &environment environment)
  (labels ((walk (rest bindings writers)
             (if (null rest)
                 `(let* ,(reverse bindings) ,@(reverse writers) nil)
                 (multiple-value-bind (vars vals stores writer)
                     (get-setf-expansion (car rest) environment)
                   (walk (cdr (cdr rest))
                         (cons (list (%one-store stores (car rest)) (car (cdr rest)))
                               (append (reverse (%bindings vars vals)) bindings))
                         (cons writer writers))))))
    (walk pairs nil nil)))

;;; Expand every place in PLACES: the bindings of all their temporaries, in
;;; order, then the store, writer and reader of each.
(defun %expand-places (places environment)
  (labels ((walk (rest bindings stores writers readers)
             (if (null rest)
                 (values bindings (reverse stores) (reverse writers) (reverse readers))
                 (multiple-value-bind (vars vals store-list writer reader)
                     (get-setf-expansion (car rest) environment)
                   (walk (cdr rest) (append bindings (%bindings vars vals))
                         (cons (%one-store store-list (car rest)) stores)
                         (cons writer writers) (cons reader readers))))))
    (walk places nil nil nil nil)))

;;; ------------------------------------------------------------------------
;;; The places themselves (decision 3)

(defsetf caar (x) (new) `(progn (rplaca (car ,x) ,new) ,new))
(defsetf cadr (x) (new) `(progn (rplaca (cdr ,x) ,new) ,new))
(defsetf cdar (x) (new) `(progn (rplacd (car ,x) ,new) ,new))
(defsetf cddr (x) (new) `(progn (rplacd (cdr ,x) ,new) ,new))
(defsetf caddr (x) (new) `(progn (rplaca (cddr ,x) ,new) ,new))
(defsetf second (x) (new) `(progn (rplaca (cdr ,x) ,new) ,new))
(defsetf third (x) (new) `(progn (rplaca (cddr ,x) ,new) ,new))
(defsetf nth %set-nth)
(defsetf elt %set-elt)
(defsetf symbol-value set)

(defun %set-nth (n list new)
  (if (= n 0)
      (progn (rplaca list new) new)
      (%set-nth (- n 1) (cdr list) new)))

(defun %set-elt (sequence index new)
  (if (listp sequence)
      (%set-nth index sequence new)
      (setf (aref sequence index) new)))

;;; `(setf (values a b) form)` stores each value of FORM in its own place:
;;; one store per place, so this is the place with several stores.
(define-setf-expander values (&rest places &environment environment)
  (labels ((walk (rest vars vals stores writers readers)
             (if (null rest)
                 (values vars vals (reverse stores)
                         (cons 'values (reverse writers))
                         (cons 'values (reverse readers)))
                 (multiple-value-bind (v1 v2 store-list writer reader)
                     (get-setf-expansion (car rest) environment)
                   (walk (cdr rest) (append vars v1) (append vals v2)
                         (cons (%one-store store-list (car rest)) stores)
                         (cons writer writers) (cons reader readers))))))
    (walk places nil nil nil nil nil)))

;;; `(setf (the type place) v)` is a store into PLACE of a value of TYPE.
(define-setf-expander the (type place &environment environment)
  (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
    (values vars vals stores writer (list 'the type reader))))

;;; ------------------------------------------------------------------------
;;; getf, the place inside a place
;;;
;;; `(setf (getf plist :colour) 'red)` changes the value after `:colour` if
;;; the list has one. If it has none, the new property goes on the *front*,
;;; which makes a new list — so the variable (or whatever place) holding
;;; `plist` must be written too. A reader → writer table cannot say that:
;;; the expander asks for the expansion of the inner place and writes
;;; through it. This is the reason `define-setf-expander` exists.

(defun getf (plist indicator &optional default)
  (cond ((null plist) default)
        ((eq (car plist) indicator) (car (cdr plist)))
        (t (getf (cdr (cdr plist)) indicator default))))

;;; PLIST with INDICATOR's value set to VALUE: the same list changed in
;;; place, or a new one with the property in front.
(defun %putf (plist indicator value)
  (labels ((walk (rest)
             (cond ((null rest) (cons indicator (cons value plist)))
                   ((eq (car rest) indicator)
                    (rplaca (cdr rest) value)
                    plist)
                   (t (walk (cdr (cdr rest)))))))
    (walk plist)))

(define-setf-expander getf (place indicator &optional (default nil default-p)
                            &environment environment)
  (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
    (let ((key (gensym "INDICATOR"))
          (fallback (gensym "DEFAULT"))
          (new (gensym "NEW")))
      (values (append vars (list key) (if default-p (list fallback) nil))
              (append vals (list indicator) (if default-p (list default) nil))
              (list new)
              `(let ((,(%one-store stores place) (%putf ,reader ,key ,new)))
                 ,writer
                 ,new)
              `(getf ,reader ,key ,@(if default-p (list fallback) nil))))))

;;; `(remf place indicator)` removes the property and says whether there was
;;; one. Removing the first property makes the list shorter at the front, so
;;; the place is written, as for `getf`.
(defmacro remf (place indicator &environment environment)
  (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
    (let ((key (gensym "INDICATOR"))
          (removed (gensym "REMOVED")))
      `(let* (,@(%bindings vars vals) (,key ,indicator))
         (multiple-value-bind (,(%one-store stores place) ,removed) (%remf ,reader ,key)
           ,writer
           ,removed)))))

;;; PLIST without INDICATOR's property, and whether it had one. The list is
;;; changed in place, as SBCL's is, except when the first property goes.
(defun %remf (plist indicator)
  (cond ((null plist) (values nil nil))
        ((eq (car plist) indicator) (values (cdr (cdr plist)) t))
        (t (labels ((walk (before)
                      (let ((rest (cdr (cdr before))))
                        (cond ((null rest) (values plist nil))
                              ((eq (car rest) indicator)
                               (rplacd (cdr before) (cdr (cdr rest)))
                               (values plist t))
                              (t (walk rest))))))
             (walk plist)))))
