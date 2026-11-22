;;;; eval, load, and the loop a host drives (#47, step D of #39).
;;;;
;;;; With the reader (#45) and the printer (#46) in Lisp, the last piece of a
;;;; read-eval-print loop is the E. Quokka never interprets in a built world
;;;; (#25): `eval` compiles its form with the image's own compiler and calls
;;;; the result, which is what SBCL does in its default `:compile` evaluator
;;;; mode.
;;;;
;;;; Decided in #47 (Carsten): the loop itself is Lisp too. A host — the REPL,
;;;; the differential harness, later the browser console — hands this file a
;;;; string with `%host-source` and calls `%host-step` until it says the input
;;;; is finished. Each call reads one form with the Lisp reader, so it is
;;;; interned through `*package*`; evaluates it; and prints the value with
;;;; `prin1`. The host only relays what it is told.

;;; `(eval form)` — compile FORM into a function of no arguments and call it.
;;;
;;; An `eval` inside a macro expander runs while another form is being
;;; compiled, and so compiles inside a compilation; `compile-form` saves and
;;; restores the outer one's work around it (#47). Two *processes* compiling
;;; at once is a different matter, and phase 3's (#34).
(defun eval (form)
  (funcall (compile-form form)))

;;; A fresh cons, so no form in a file can be mistaken for the end of it.
(defvar %end-of-input (list :end-of-input))

;;; `(load source)` — every form of SOURCE, read and evaluated in order. A
;;; string names a file, which is fetched through the host (`%read-source`,
;;; #35); an input stream is read directly. `*package*` and `*readtable*` are
;;; rebound for the duration, as CLHS says, so an `in-package` in a file stays
;;; in that file. CLHS's keyword arguments — `:verbose`, `:print`,
;;; `:if-does-not-exist` — are not here yet.
(defun load (source)
  (let ((stream (if (stringp source)
                    (make-string-input-stream (%read-source source))
                    source))
        (*package* *package*)
        (*readtable* *readtable*))
    (%load-forms stream)
    t))

(defun %load-forms (stream)
  (let ((form (read stream nil %end-of-input)))
    (if (eq form %end-of-input)
        nil
        (progn (eval form)
               (%load-forms stream)))))

;;; ------------------------------------------------------------------------
;;; The host's loop

(defvar %host-input (make-string-input-stream ""))

;;; Give the loop its input: the line a person typed, or a whole file.
(defun %host-source (text)
  (setq %host-input (make-string-input-stream text))
  t)

;;; Read, evaluate and print one form, and say what happened:
;;;
;;;   (:value "printed value")   the form's value, as `prin1` writes it
;;;   (:error "message")         a Lisp error, caught here
;;;   (:eof)                     nothing left to read
;;;
;;; An error is caught so that the host can carry on with the next form, as a
;;; REPL does. What cannot be caught here — the step budget running out — the
;;; host sees as a failure of the call itself. The argument is ignored: the
;;; host's trampoline always passes one.
(defun %host-step (ignored)
  (handler-case
      (let ((form (read %host-input nil %end-of-input)))
        (if (eq form %end-of-input)
            (list :eof)
            (list :value (prin1-to-string (eval form)))))
    (error (condition) (list :error (%condition-text condition)))))

;;; A condition's message as text, for a person to read.
(defun %condition-text (condition)
  (let ((message (%condition-message condition)))
    (if (stringp message) message (princ-to-string message))))
