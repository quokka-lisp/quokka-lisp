;;;; The ansi-test runner (#60, step N of #39).
;;;;
;;;; ansi-test's own harness, rt.lsp, needs loop, handler-bind, typep and
;;;; compile-file, which tier 1 does not have (#61 is the upgrade to it). This
;;;; is a small replacement in tier-1 Lisp, loaded into a world by the driver
;;;; (crates/quokka-bootstrap/tests/ansi.rs). The test files themselves are
;;;; read unmodified.
;;;;
;;;; The driver opens one file at a time with `(rt::%open name)` and then
;;;; calls `(rt::%step)` until it says "DONE". Each step reads one top-level
;;;; form and either runs it as a test or loads it as a definition, and
;;;; returns one line saying what happened. Each step is a separate call with
;;;; its own step budget, so a test that never finishes fails alone.
;;;;
;;;; A step's line is one of:
;;;;
;;;;   PASS name                 the test's values were the expected ones
;;;;   FAIL name detail          they were not, or the test signalled
;;;;   NEEDS name reason...      the test uses something Quokka lacks (decision 4)
;;;;   UNREADABLE name reason    Quokka's reader cannot read the test
;;;;   SKIP name reason          the test is switched off by a disabled note
;;;;   LOAD what reason          a definition, and what became of it
;;;;   DONE                      the file is finished

(defpackage :regression-test
  (:use :common-lisp)
  (:nicknames :rt :rtest)
  (:export :deftest :*test* :*compile-tests* :*catch-errors* :do-test :do-tests
           :rem-test :get-test :defnote :disable-note :enable-note :my-aref))

(defpackage :cl-test
  (:use :common-lisp :regression-test)
  (:export :signals-error :random-from-seq :random-case :coin :random-permute
           :*universe* :*mini-universe* :*cl-symbols* :typef))

(in-package :regression-test)

(defvar *test* nil)

;;; The test being run, for the driver to name when the step budget runs
;;; out: a global, since an abandoned computation unwinds every binding.
(defvar *current* "-")
(defvar *compile-tests* nil)
(defvar *catch-errors* t)

;;; ------------------------------------------------------------------------
;;; What Quokka has, and what the suite has defined

;;; The names of COMMON-LISP's operators by SBCL's account, from
;;; cl-operators.lisp, and the same as a table of symbols.
(defvar *cl-operator-names* nil)
(defvar *cl-operators* (make-hash-table :test 'eq))

;;; Operators Quokka lacks on purpose, each with its reason: the deferred
;;; tier or issue (deferred.lisp). One Quokka lacks and that is not here is a
;;; gap in tier 1.
(defvar *deferred* (make-hash-table :test 'eq))

;;; Helpers the suite defines that could not be defined here, each with the
;;; reasons why. A test that uses one inherits the reasons.
(defvar *broken* (make-hash-table :test 'eq))

;;; Helpers this runner provides itself, which the suite's own definitions
;;; must not replace: they need handler-bind, typep or compile.
(defvar *provided* (make-hash-table :test 'eq))

;;; ansi-test's notes: a name, and whether it is disabled.
(defvar *disabled-notes* nil)

(defun %prepare ()
  (dolist (name *cl-operator-names*)
    (setf (gethash (find-symbol name "COMMON-LISP") *cl-operators*) t))
  t)

(defun %deferred (reason &rest names)
  (dolist (name names)
    (setf (gethash (find-symbol (string name) "COMMON-LISP") *deferred*) reason)))

;;; Does Quokka have OPERATOR as a function, a macro or a special operator?
;;; SYMBOL-MACROLET is the one special operator only the compiler knows
;;; (#53), so the table of special operators does not list it.
(defun %operator-p (symbol)
  (or (quokka::%special-operator-p symbol)
      (eq symbol 'symbol-macrolet)
      (quokka::%macro-function symbol)
      (gethash symbol *provided*)
      (if (ignore-errors (symbol-function symbol)) t nil)))

;;; ------------------------------------------------------------------------
;;; What a form needs (decision 4)

;;; The reasons FORM cannot work in Quokka: a COMMON-LISP operator Quokka
;;; does not have, wherever it appears — in the operator position, under
;;; #', or quoted inside a macro's expansion — and the reasons of every
;;; broken helper it mentions. Nil if there are none. A circular form (#1=)
;;; would be walked for ever, so the walk stops after *walk-limit* nodes.
;;; (An `eq` table of the conses met could do it exactly since #64; a limit
;;; has been enough.)
(defvar *walk-limit* 20000)

(defun %needs (form)
  (let ((left *walk-limit*)
        (reasons nil))
    (labels ((walk (x)
               (setq left (- left 1))
               (cond ((< left 0) nil)
                     ((symbolp x)
                      (dolist (r (%symbol-need x)) (pushnew r reasons :test #'string=)))
                     ((consp x) (walk (car x)) (walk (cdr x)))
                     ((and (vectorp x) (not (stringp x)))
                      (dotimes (i (length x)) (walk (aref x i)))))))
      (walk form))
    (sort reasons #'string<)))

;;; A helper's needs are its definition's, found when a test first uses it
;;; rather than when it is loaded: a helper may call one defined after it
;;; (APPROX= calls EPSILON, which comes later and needs complex numbers).
;;; Remembered once found; a helper in the middle of being looked at — a
;;; recursive one — counts as needing nothing more.
(defvar *definitions* (make-hash-table :test 'eq))
(defvar *helper-needs* (make-hash-table :test 'eq))

(defun %symbol-need (symbol)
  (cond ((null symbol) nil)
        ((gethash symbol *broken*))
        ((and (gethash symbol *cl-operators*) (not (%operator-p symbol)))
         (list (%reason-for symbol)))
        ((gethash symbol *definitions*) (%helper-need symbol))
        (t nil)))

(defun %helper-need (symbol)
  (multiple-value-bind (known found) (gethash symbol *helper-needs*)
    (if found
        known
        (progn (setf (gethash symbol *helper-needs*) nil)
               (setf (gethash symbol *helper-needs*)
                     (%needs (gethash symbol *definitions*)))))))

(defun %reason-for (symbol)
  (let ((deferred (gethash symbol *deferred*)))
    (if deferred
        (format nil "~A[~A]" (symbol-name symbol) deferred)
        (format nil "~A[tier-1]" (symbol-name symbol)))))

;;; ------------------------------------------------------------------------
;;; Comparing values, as rt.lsp's EQUALP-WITH-CASE does: like EQUALP, but
;;; characters compared with their case, and numbers with EQL.

(defun %same (x y)
  (cond ((eq x y) t)
        ((consp x) (and (consp y) (%same (car x) (car y)) (%same (cdr x) (cdr y))))
        ((and (vectorp x) (vectorp y))
         (and (= (length x) (length y))
              (let ((same t))
                (dotimes (i (length x) same)
                  (if (not (%same (aref x i) (aref y i))) (setq same nil))))))
        ((and (stringp x) (stringp y)) (string= x y))
        ;; Arrays of other ranks (#67): the same dimensions, and the same
        ;; elements in row-major order, as rt.lsp's EQUALP-WITH-CASE.
        ((and (arrayp x) (arrayp y))
         (and (equal (array-dimensions x) (array-dimensions y))
              (let ((same t))
                (dotimes (i (array-total-size x) same)
                  (if (not (%same (row-major-aref x i) (row-major-aref y i))) (setq same nil))))))
        (t (eql x y))))

;;; ------------------------------------------------------------------------
;;; Reading a file

(defvar *stream* nil)
(defvar *file* nil)
(defvar *file-package* nil)

(defun %open (name)
  (setq *stream* (make-string-input-stream (quokka::%read-source name))
        *file* name
        *file-package* (find-package "CL-TEST"))
  t)

(defun %position () (quokka::%string-input-stream-position *stream*))
(defun %set-position (at) (setf (quokka::%string-input-stream-position *stream*) at))
(defun %text () (quokka::%string-input-stream-string *stream*))

;;; The next form, or :eof, or (values :unreadable message name) when the
;;; reader refuses it — after which the stream is moved past the form by a
;;; crude scanner that knows parentheses, strings, |symbols|, characters and
;;; comments, which is enough to find where a form ends.
(defvar *eof* (list :eof))

(defun %read-next ()
  (%skip-blank)
  (let ((start (%position)))
    (handler-case (let ((form (read *stream* nil *eof*)))
                    (if (eq form *eof*) :eof form))
      (error (condition)
        condition
        (%set-position start)
        (let ((name (%deftest-name-at start)))
          (%skip-form)
          (let ((reason (%unreadable-reason (subseq (%text) start (%position)))))
            (%break-unreadable-definition start reason)
            (values :unreadable reason name)))))))

;;; Why the reader refused TEXT, as "needs" reasons. If it reads once `#.`
;;; is no longer evaluated, the evaluation is what failed, and the form it
;;; would have evaluated says what is missing — `#.(find-class 'symbol)`
;;; needs `find-class` (step Y, #72). Otherwise it is syntax.
(defun %unreadable-reason (text)
  (let* ((form (%read-without-eval text))
         (needs (if form (%needs form) nil)))
    (if needs (format nil "~{~A~^ ~}" needs) (%syntax-reason text))))

(defun %read-without-eval (text)
  (let ((*readtable* (copy-readtable)))
    (set-dispatch-macro-character #\# #\. (lambda (stream char argument)
                                             char argument
                                             (read stream t nil t)))
    (ignore-errors (read-from-string text))))

;;; The syntax in TEXT that Quokka's reader lacks, or "reader[other]". Only
;;; what is still missing is probed for: searching the text cannot tell
;;; which of several kinds of syntax made the reader give up, so a probe for
;;; syntax the reader has blames it for a neighbour's failure (#72).
(defun %syntax-reason (text)
  (let ((found nil))
    (dolist (probe '(("#c(" . "#C") ("#C(" . "#C") ("#p\"" . "#P") ("#P\"" . "#P")))
      (if (and (search (car probe) text) (not (member (cdr probe) found :test #'string=)))
          (push (cdr probe) found)))
    (if (%has-ratio text) (push "ratio" found))
    (if found
        (format nil "~{reader[~A]~^ ~}" (sort found #'string<))
        "reader[other]")))

;;; Digits, a slash, digits: the syntax of a ratio, which Quokka reads as
;;; an error until #59.
(defun %has-ratio (text)
  (let ((found nil))
    (dotimes (i (- (length text) 2) found)
      (if (and (digit-char-p (char text i)) (char= (char text (+ i 1)) #\/)
               (digit-char-p (char text (+ i 2))))
          (setq found t)))))

;;; An unreadable DEFUN, DEFMACRO, DEFVAR or DEFPARAMETER still defines a
;;; name, which tests call; mark it broken with the reader's reason.
(defun %break-unreadable-definition (start reason)
  (let* ((text (%text))
         (head (string-upcase (subseq text start (min (length text) (+ start 120))))))
    (dolist (kind '("(DEFUN " "(DEFMACRO " "(DEFVAR " "(DEFPARAMETER " "(DEFCONSTANT "))
      (if (and (> (length head) (length kind))
               (string= (subseq head 0 (length kind)) kind))
          (let* ((from (length kind))
                 (to (or (position-if (lambda (c) (member c '(#\Space #\Newline #\Tab #\) #\()))
                                      head :start from)
                         (length head))))
            (%break (list (intern (subseq head from to))) (list reason)))))))

(defun %condition-text (condition)
  (let ((message (quokka::%condition-message condition)))
    (if (stringp message) message (princ-to-string message))))

(defun %peek () (if (< (%position) (length (%text))) (char (%text) (%position)) nil))
(defun %advance () (%set-position (+ (%position) 1)))

(defun %skip-blank ()
  (let ((c (%peek)))
    (cond ((null c) nil)
          ((member c '(#\Space #\Tab #\Newline #\Return #\Page))
           (%advance) (%skip-blank))
          ((char= c #\;) (%skip-line) (%skip-blank))
          ((and (char= c #\#) (%next-is #\|)) (%skip-block-comment) (%skip-blank))
          (t nil))))

(defun %next-is (char)
  (let ((at (+ (%position) 1)))
    (and (< at (length (%text))) (char= (char (%text) at) char))))

(defun %skip-line ()
  (do () ((or (null (%peek)) (char= (%peek) #\Newline))) (%advance)))

(defun %skip-block-comment ()
  (%advance) (%advance)
  (let ((depth 1))
    (do () ((or (null (%peek)) (= depth 0)))
      (cond ((and (char= (%peek) #\|) (%next-is #\#)) (%advance) (%advance) (setq depth (- depth 1)))
            ((and (char= (%peek) #\#) (%next-is #\|)) (%advance) (%advance) (setq depth (+ depth 1)))
            (t (%advance))))))

;;; Skip one form's text. Prefix characters (' ` , @ #) are skipped with the
;;; form after them; #+ and #- skip their feature expression too.
(defun %skip-form ()
  (%skip-blank)
  (let ((c (%peek)))
    (cond ((null c) nil)
          ((member c '(#\' #\` #\,)) (%advance) (if (eql (%peek) #\@) (%advance)) (%skip-form))
          ((char= c #\() (%advance) (%skip-list))
          ((char= c #\)) (%advance))
          ((char= c #\") (%advance) (%skip-delimited #\"))
          ((char= c #\|) (%advance) (%skip-delimited #\|) (%skip-token))
          ((char= c #\#)
           (%advance)
           (do () ((not (and (%peek) (digit-char-p (%peek))))) (%advance))
           (let ((d (%peek)))
             (cond ((null d) nil)
                   ((char= d #\\) (%advance) (%advance) (%skip-token))
                   ((member d '(#\+ #\-)) (%advance) (%skip-form) (%skip-form))
                   ((char= d #\() (%skip-form))
                   ((member d '(#\' #\. #\: #\= #\A #\a #\C #\c #\P #\p #\S #\s))
                    (%advance) (%skip-form))
                   (t (%advance) (%skip-token)))))
          (t (%skip-token)))))

(defun %skip-list ()
  (%skip-blank)
  (cond ((null (%peek)) nil)
        ((char= (%peek) #\)) (%advance))
        (t (%skip-form) (%skip-list))))

(defun %skip-delimited (close)
  (do () ((or (null (%peek)) (char= (%peek) close)) (%advance))
    (if (char= (%peek) #\\) (%advance))
    (%advance)))

(defun %skip-token ()
  (do () ((or (null (%peek))
              (member (%peek) '(#\Space #\Tab #\Newline #\Return #\Page #\( #\) #\" #\' #\` #\, #\;))))
    (if (char= (%peek) #\\) (%advance))
    (if (char= (%peek) #\|) (progn (%advance) (%skip-delimited #\|)) (%advance))))

;;; If the text at START is `(deftest NAME`, the name as it is written.
(defun %deftest-name-at (start)
  (let* ((text (%text))
         (end (min (length text) (+ start 200)))
         (head (string-upcase (subseq text start end))))
    (if (and (> (length head) 9) (string= (subseq head 0 9) "(DEFTEST "))
        (let* ((from 9)
               (to (or (position-if (lambda (c) (member c '(#\Space #\Newline #\Tab #\) #\()))
                                    head :start from)
                       (length head))))
          (subseq head from to))
        nil)))

;;; ------------------------------------------------------------------------
;;; One step

(defun %step ()
  (setq *current* "-")
  (let ((*package* *file-package*)
        (*produced* nil))
    (multiple-value-bind (form reason name) (%read-next)
      (let ((line (cond ((eq form :eof) "DONE")
                        ((eq form :unreadable)
                         (if name
                             (format nil "UNREADABLE ~A ~A" name (%one-line reason))
                             (format nil "LOAD unreadable ~A" (%one-line reason))))
                        ((and (consp form) (eq (car form) 'deftest))
                         (handler-case (%run-test form)
                           (error (condition)
                             (format nil "FAIL ~A runner-error ~A" (second form)
                                     (%one-line (%condition-text condition))))))
                        (t (handler-case (%load-form form)
                             (error (condition)
                               (format nil "LOAD runner-error ~A"
                                       (%one-line (%condition-text condition)))))))))
        (setq *file-package* *package*)
        (format nil "~A~{~%~A~}" line (reverse *produced*))))))

(defun %one-line (text)
  (let ((text (substitute #\Space #\Newline (princ-to-string text))))
    (if (> (length text) 160) (concatenate 'string (subseq text 0 160) "...") text)))

;;; ------------------------------------------------------------------------
;;; Running a test

(defun %run-test (form)
  (let* ((name (second form))
         (rest (cddr form))
         (properties nil))
    (do () ((not (keywordp (car rest))))
      (push (list (car rest) (cadr rest)) properties)
      (setq rest (cddr rest)))
    (let* ((test (car rest))
           (expected (cdr rest))
           (notes (cadr (assoc :notes properties)))
           (disabled (%disabled-note (if (listp notes) notes (list notes))))
           (*test* name))
      (setq *current* (symbol-name name))
      (if disabled
          (format nil "SKIP ~A disabled note ~A" name disabled)
          (let ((values (handler-case (multiple-value-list (eval test))
                          (error (condition) (list :signalled (%condition-text condition))))))
            (if (and (not (and (consp values) (eq (car values) :signalled)))
                     (%same values expected))
                (format nil "PASS ~A" name)
                ;; Every test runs; what it needs is asked only once it has
                ;; failed, to explain the failure (decision 4). Asking first
                ;; would skip tests that pass, such as one using a helper that
                ;; needs LOOP only in a branch the test does not take.
                (let ((needs (%needs test)))
                  (if needs
                      (format nil "NEEDS ~A ~{~A~^ ~}" name needs)
                      (format nil "FAIL ~A ~A" name (%one-line (%describe values)))))))))))

;;; A test made by a macro of the suite's (DEF-MACRO-TEST and the like)
;;; arrives inside another form, not at the top level. DEFTEST runs it there
;;; and keeps its line for the step to report.
(defvar *produced* nil)

(defmacro deftest (name &rest body)
  `(progn (push (%run-test '(deftest ,name ,@body)) *produced*) ',name))

(defun %describe (values)
  (or (ignore-errors (prin1-to-string values)) "#<unprintable>"))

(defun %disabled-note (notes)
  (dolist (note notes nil)
    (if (member note *disabled-notes*) (return note))))

;;; ------------------------------------------------------------------------
;;; Loading a definition

;;; The names a top-level form defines, for the table of broken helpers.
(defun %defined-names (form)
  (if (not (consp form))
      nil
      (let ((head (car form)))
        (cond ((member head '(defun defmacro defgeneric defvar defparameter defconstant
                              defsetf define-modify-macro))
               (let ((name (second form)))
                 (if (and (consp name) (eq (car name) 'setf)) nil (list name))))
              ((eq head 'defstruct) (%struct-names (second form) (cddr form)))
              ((member head '(progn eval-when locally let let*))
               (let ((names nil))
                 (dolist (sub (cdr form) names)
                   (setq names (append (%defined-names sub) names)))))
              (t nil)))))

(defun %struct-names (spec slots)
  (let* ((name (if (consp spec) (car spec) spec))
         (prefix (concatenate 'string (symbol-name name) "-"))
         (names (list name
                      (intern (concatenate 'string "MAKE-" (symbol-name name)))
                      (intern (concatenate 'string "COPY-" (symbol-name name)))
                      (intern (concatenate 'string (symbol-name name) "-P")))))
    (dolist (slot slots names)
      (let ((slot-name (if (consp slot) (car slot) slot)))
        (if (symbolp slot-name)
            (push (intern (concatenate 'string prefix (symbol-name slot-name))) names))))))

;;; Load a definition, unless it redefines something in COMMON-LISP (which
;;; would replace Quokka's own) or something this runner provides. One that
;;; signals marks what it defines as broken, with what it needs — or, if it
;;; needs nothing Quokka lacks, with the load error — so that the tests using
;;; it are reported as needing that. One that loads is remembered, for the
;;; same question asked later.
(defun %load-form (form)
  (let ((names (%defined-names form)))
    (cond ((and names (%common-lisp-p (car names)))
           (format nil "LOAD ~A not redefined: it is COMMON-LISP's" (car names)))
          ((and names (gethash (car names) *provided*))
           (format nil "LOAD ~A provided by the runner" (car names)))
          (t
           (let ((failure (handler-case (progn (eval form) nil)
                            (error (condition) (%condition-text condition)))))
             (if failure
                 (let ((needs (or (%needs form)
                                  (list (format nil "load-error[~A]" (or (car names) "-"))))))
                   (%break names needs)
                   (format nil "LOAD ~A failed ~A: needs ~{~A~^ ~}"
                           (or names "-") (%one-line failure) needs))
                 (progn (%remember names form)
                        (format nil "LOAD ~A ok" (or names "-")))))))))

;;; A definition that loaded, kept so that its needs can be found later.
(defun %remember (names form)
  (setq *helper-needs* (make-hash-table :test 'eq))
  (dolist (name names)
    (if (symbolp name)
        (progn (setf (gethash name *definitions*) form)
               (remhash name *broken*)))))

(defun %common-lisp-p (symbol)
  (and (symbolp symbol) (eq (symbol-package symbol) (find-package "COMMON-LISP"))))

(defun %break (names reasons)
  (setq *helper-needs* (make-hash-table :test 'eq))
  (dolist (name names)
    (if (symbolp name) (setf (gethash name *broken*) reasons))))

;;; ------------------------------------------------------------------------
;;; ansi-test's notes (notes.lsp)

(defmacro defnote (name contents &optional disabled)
  contents
  (if disabled `(pushnew ',name *disabled-notes*) `',name))

(defun disable-note (name &optional errorp)
  errorp
  (pushnew name *disabled-notes*))

(defun enable-note (name &optional errorp)
  errorp
  (setq *disabled-notes* (remove name *disabled-notes*)))

(defun my-aref (array &rest indices) (aref array (car indices)))

;;; ------------------------------------------------------------------------
;;; Helpers the runner provides (decision 4)
;;;
;;; ansi-aux.lsp's versions of these use handler-bind, proclaim and typep.
;;; These keep their meaning with what tier 1 has: SIGNALS-ERROR is true
;;; when evaluating FORM signals a condition of type ERROR-NAME. Quokka's
;;; condition types have no hierarchy yet, so only `error` itself matches
;;; every error, and any other name matches exactly — a test expecting
;;; TYPE-ERROR where Quokka signals SIMPLE-ERROR fails, as it should.

(in-package :cl-test)

(defmacro signals-error (form error-name &key safety name inline)
  safety name inline
  `(handler-case (progn (eval ',form) nil)
     (,error-name (condition) condition t)))

(defmacro signals-error-always (form error-name)
  `(values (signals-error ,form ,error-name) (signals-error ,form ,error-name)))

(defmacro catch-type-error (form)
  `(handler-case ,form
     (type-error () 'type-error)
     (error (condition) condition)))

(defmacro classify-error* (form)
  `(handler-case ,form
     (undefined-function () 'undefined-function)
     (program-error () 'program-error)
     (package-error () 'package-error)
     (type-error () 'type-error)
     (control-error () 'control-error)
     (parse-error () 'parse-error)
     (stream-error () 'stream-error)
     (reader-error () 'reader-error)
     (file-error () 'file-error)
     (cell-error () 'cell-error)
     (division-by-zero () 'division-by-zero)
     (floating-point-overflow () 'floating-point-overflow)
     (floating-point-underflow () 'floating-point-underflow)
     (arithmetic-error () 'arithmetic-error)
     (error () 'error)))

(defmacro classify-error (form)
  `(classify-error* (eval ',form)))

(in-package :regression-test)

(dolist (name '(cl-test::signals-error cl-test::signals-error-always cl-test::catch-type-error
                cl-test::classify-error* cl-test::classify-error deftest defnote
                disable-note enable-note my-aref))
  (setf (gethash name *provided*) t))
