;;;; Refuse to bind a COMMON-LISP symbol in a differential test file.
;;;;
;;;; Binding or redefining an external symbol of the COMMON-LISP package is
;;;; undefined behaviour — CLHS 11.1.2.1.2 — and SBCL enforces it with a
;;;; package lock. Quokka has no packages and allows it, so the two disagree
;;;; for a reason that is the test's fault.
;;;;
;;;; tests/lisp/README.md says so, and the rule has been broken twice: once
;;;; with CAR, which looks like a Common Lisp symbol, and once with LOG, which
;;;; does not. That is the problem. The standard's thousand external symbols
;;;; include `log`, `count`, `search`, `last`, `merge`, `values`, `step` and
;;;; `time` — ordinary words that nobody checks before using as a variable.
;;;;
;;;; A rule in prose is a hope. This is the same rule where it fails the build,
;;;; with a message naming the symbol rather than a diff of fifteen lines.

(setf *print-pretty* nil)

(defun cl-symbol-p (s)
  (and (symbolp s)
       (not (null s))
       (not (eq s t))
       (multiple-value-bind (found status) (find-symbol (symbol-name s) :cl)
         (and found (eq status :external) (eq found s)))))

(defvar *offences* nil)

(defun note (symbol context)
  (pushnew (list symbol context) *offences* :test #'equal))

(defun proper-list-p (x)
  (loop (cond ((null x) (return t))
              ((consp x) (setf x (cdr x)))
              (t (return nil)))))

(defun check-bindings (bindings context)
  "BINDINGS is a let/flet-style list; each element binds its first symbol."
  (when (proper-list-p bindings)
    (dolist (b bindings)
      (let ((name (if (consp b) (car b) b)))
        (when (cl-symbol-p name) (note name context))))))

(defun check-head (form)
  "Look at FORM itself, without descending."
  (let ((head (car form)))
    (when (symbolp head)
      (case head
        ((setq setf)
         (when (proper-list-p (cdr form))
           (loop for rest on (cdr form) by #'cddr
                 for name = (car rest)
                 when (cl-symbol-p name) do (note name "assigned"))))
        ((defun defmacro block return-from)
         (let ((name (and (consp (cdr form)) (second form))))
           (when (cl-symbol-p name) (note name (string-downcase (string head))))))
        ((let let* flet labels macrolet)
         (when (consp (cdr form))
           (check-bindings (second form) (string-downcase (string head)))))
        (t nil)))))

(defun walk (form)
  "Walk every cons, including improper ones. Total by construction.

An earlier version used REMOVE-IF-NOT and DOLIST, which need proper lists, and
died on the literal (1 . 2) in lists.lisp — a checker that crashes on valid
input is worse than no checker, because its failure looks like the thing it was
meant to detect."
  (when (consp form)
    ;; Quoted data cannot contain a binding, and walking into it would report
    ;; '(setq log 1) written as data.
    (unless (and (symbolp (car form)) (member (car form) '(quote function)))
      (check-head form)
      (walk (car form))
      (walk (cdr form)))))

(defun check-file (path)
  (let ((*offences* nil)
        (eof (list :eof)))
    (with-open-file (in path :direction :input)
      (loop for form = (handler-case (read in nil eof) (error () eof))
            until (eq form eof)
            do (walk form)))
    (when *offences*
      (format *error-output* "~&~a binds COMMON-LISP symbols:~%" path)
      (dolist (o (reverse *offences*))
        (format *error-output* "    ~a (~a) — undefined behaviour; use a name of your own~%"
                (first o) (second o)))
      t)))

(let ((bad nil))
  (dolist (path (directory "tests/lisp/*.lisp"))
    (when (check-file path) (setf bad t)))
  (unless bad
    (format t "names: no COMMON-LISP symbols bound in any test file~%"))
  (sb-ext:exit :code (if bad 1 0)))
