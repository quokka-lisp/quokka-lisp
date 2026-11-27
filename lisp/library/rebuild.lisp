;;;; The image rebuilds itself (#48, step E of #39).
;;;;
;;;; The cold build, `boot::build_image`, makes the first world from nothing,
;;;; in Rust, with the interpreter. This is the warm rebuild: a running
;;;; compiled world reads its own source with its own reader, compiles it with
;;;; its own compiler, and replaces every function with the new code. What it
;;;; saves can do the same again. In #39's terms the cold build makes C1, a
;;;; rebuild in C1 makes C2, and a rebuild in C2 makes C3; step F compares
;;;; them.
;;;;
;;;; Decided in #48 (Carsten):
;;;;
;;;; - The source comes **from the host, by name**, through `%read-source`,
;;;;   as `load`'s does: files on disk for the native REPL, so an edited
;;;;   compiler is rebuilt without recompiling any Rust; the copies the binary
;;;;   embeds for the cold build, in tests.
;;;; - **Two passes, as the cold build does.** Every form of the prelude and
;;;;   the compiler is compiled by the current compiler before any of them is
;;;;   installed, so the new compiler is entirely the old compiler's work.
;;;;   Replacing a compiler while it compiles itself would make a late
;;;;   function the work of a half-replaced compiler — harmless when the two
;;;;   agree, and the hardest place to look when they don't. The library
;;;;   follows form by form, compiled by the new compiler, because it defines
;;;;   macros it then uses.
;;;;
;;;; The source is read in `QUOKKA`, the package the cold build put the
;;;; system's own symbols in, so `%compile` names the function the running
;;;; system already calls, and `defun` and `car` come from `COMMON-LISP`.

;;; The system's source, by the names the host knows it by. The library is
;;; in `boot::LIBRARY`'s order, which a test holds this list to.
(defvar %prelude-file "lisp/prelude.lisp")
(defvar %compiler-files '("lisp/compiler/compile.lisp"))
(defvar %library-files
  '("lisp/library/specials.lisp" "lisp/library/cl-symbols.lisp"
    "lisp/library/numbers.lisp" "lisp/library/operators.lisp"
    "lisp/library/setf.lisp" "lisp/library/lists.lisp"
    "lisp/library/symbols.lisp"
    "lisp/library/sequences.lisp" "lisp/library/hash-tables.lisp"
    "lisp/library/strings.lisp"
    "lisp/library/arrays.lisp"
    "lisp/library/structures.lisp"
    "lisp/library/integers.lisp" "lisp/library/floats.lisp" "lisp/library/ratios.lisp"
    "lisp/library/types.lisp" "lisp/library/type-algebra.lisp"
    "lisp/library/clos.lisp"
    "lisp/library/generic-functions.lisp" "lisp/library/mop.lisp"
    "lisp/library/char-names.lisp" "lisp/library/reader.lisp"
    "lisp/library/printer.lisp" "lisp/library/format.lisp" "lisp/library/describe.lisp"
    "lisp/library/conditions.lisp" "lisp/library/definitions.lisp"
    "lisp/library/frames.lisp"
    "lisp/library/eval.lisp"
    "lisp/library/rebuild.lisp" "lisp/library/packages.lisp"))

;;; `(%rebuild-world)` — rebuild every function from source, and say how many
;;; forms that took and how many names it replaced.
(defun %rebuild-world ()
  (let ((*package* %quokka-package)
        ;; Every rebuild numbers its gensyms alike, so that two generations
        ;; are the same size (step Y, #72; see `gensym`).
        (*gensym-counter* 0)
        ;; The system's own definitions are its files', not recorded as
        ;; the world's (#55; `definitions.lisp`).
        (%recording-definitions nil)
        (old (%current-definitions))
        (defined nil)
        (forms 0))
    ;; Pass 1: compile the prelude and the compiler, installing nothing.
    (let ((compiled nil))
      (%for-each-form (cons %prelude-file %compiler-files)
                      (lambda (form)
                        (setq defined (%note-definition form defined))
                        (setq compiled (cons (compile-form form) compiled))))
      ;; Pass 2: install them, in file order.
      (%run-in-order (reverse compiled))
      (setq forms (length compiled)))
    ;; The library, by the compiler just installed: each form compiled and run
    ;; before the next is read, as `load` does.
    (%for-each-form %library-files
                    (lambda (form)
                      (setq defined (%note-definition form defined))
                      (setq forms (+ forms 1))
                      (eval form)))
    ;; `defvar` does not reassign, so the current readtable still holds the
    ;; old generation's reader macros. A rebuilt world starts with the
    ;; standard one, which the reloaded `reader.lisp` filled with new ones.
    (setq *readtable* (copy-readtable %standard-readtable))
    (%check-replaced defined old)
    (list :forms forms :replaced (length defined))))

;;; Call FUNCTION on every form of every file in FILES, in order, each read
;;; with the Lisp reader after the previous form has been dealt with.
(defun %for-each-form (files function)
  (if files
      (progn (%for-each-form-of (make-string-input-stream (%read-source (car files))) function)
             (%for-each-form (cdr files) function))))

(defun %for-each-form-of (stream function)
  (let ((form (read stream nil %end-of-input)))
    (if (eq form %end-of-input)
        nil
        (progn (funcall function form)
               (%for-each-form-of stream function)))))

(defun %run-in-order (functions)
  (if functions
      (progn (funcall (car functions))
             (%run-in-order (cdr functions)))))

;;; ------------------------------------------------------------------------
;;; The proof
;;;
;;; The cold build ends by scanning the heap for anything interpreted. The
;;; rebuild's equivalent: every name the source defines with `defun` or
;;; `defmacro` must hold a different function afterwards. One that still
;;; holds its old function was not rebuilt, and the rebuild fails, naming it.

;;; A name defined by FORM, added to NAMES.
(defun %note-definition (form names)
  (if (and (consp form) (or (eq (car form) 'defun) (eq (car form) 'defmacro))
           (symbolp (cadr form)))
      (cons (cadr form) names)
      names))

;;; Every symbol in `COMMON-LISP` and `QUOKKA`, with what its function cell
;;; holds now: what a rebuild must replace.
(defun %current-definitions ()
  (let ((table (make-hash-table)))
    (%note-functions (%package-externals %common-lisp-package) table)
    (%note-functions (%package-internals %quokka-package) table)
    table))

(defun %note-functions (symbols table)
  (maphash (lambda (name symbol)
             (let ((definition (ignore-errors (symbol-function symbol))))
               (if definition (setf (gethash symbol table) definition))))
           symbols))

(defun %check-replaced (names old)
  (let ((stale (%stale-names names old nil)))
    (if stale
        (error (%string-append "the rebuild left old definitions of "
                               (prin1-to-string stale))))))

(defun %stale-names (names old stale)
  (if (null names)
      stale
      (%stale-names (cdr names) old
                    (if (eq (gethash (car names) old)
                            (ignore-errors (symbol-function (car names))))
                        (cons (car names) stale)
                        stale))))
