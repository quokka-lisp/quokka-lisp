;;;; Packages (#41, #43).
;;;;
;;;; A package is a named table of symbols, so that `CL-USER::FOO` and
;;;; `QUOKKA::FOO` can be different symbols. Its *external* symbols are its
;;;; public face (`pkg:sym`); the rest are internal (`pkg::sym`); and a package
;;;; may *use* others, which makes their external symbols visible without a
;;;; prefix. CLHS chapter 11.
;;;;
;;;; Decided in #41: packages are Lisp objects here, not Rust structures. The
;;;; Rust reader and Rust's `intern` keep their one flat list — the *system
;;;; symbol table* — so the bootstrap and the tier-0 interpreter never learn
;;;; about packages. When this file has loaded, its last form *adopts* every
;;;; symbol in that table into a package, and from then on a symbol's home is
;;;; the package it belongs to.
;;;;
;;;; `eq` compares homes against 0 and 1 below where `eql` would be idiomatic:
;;;; Quokka has no `eql` yet, and until bignums and floats exist every number is
;;;; an immediate fixnum, for which the two agree. `eql` arrives with bignums
;;;; (step L), which is where they start to differ.
;;;;
;;;; MUST BE THE LAST FILE OF THE LIBRARY: only symbols that exist when the
;;;; adoption runs are adopted. Until the Lisp reader (step C) exists, every
;;;; library file is read by the Rust reader into the system table.

;;; ------------------------------------------------------------------------
;;; The structure

;;; A package. `internals` and `externals` map a symbol's *name* to the symbol,
;;; in `equal` hash tables, because a name is a string and an `equal` table is
;;; the one that hashes strings by content.
(defstruct %package name nicknames internals externals uses)

(defun packagep (object) (%package-p object))

;;; Every package, newest first. The order `list-all-packages` gives is not
;;; specified by the standard, so nothing should depend on it.
(defvar %all-packages nil)

;;; ------------------------------------------------------------------------
;;; Names and designators

;;; A *package designator* or *string designator*: a string is itself, a
;;; symbol stands for its name — so `:cl`, `cl` and `"CL"` all name the same
;;; package.
(defun %designator-name (designator)
  (cond ((stringp designator) designator)
        ((symbolp designator) (symbol-name designator))
        (t (error "a package or symbol name must be a string or a symbol"))))

(defun %designator-names (designators)
  (if (endp designators)
      nil
      (cons (%designator-name (car designators))
            (%designator-names (cdr designators)))))

(defun %string-member (name strings)
  (cond ((endp strings) nil)
        ((string= name (car strings)) t)
        (t (%string-member name (cdr strings)))))

(defun %package-named (name packages)
  (cond ((endp packages) nil)
        ((or (string= name (%package-name (car packages)))
             (%string-member name (%package-nicknames (car packages))))
         (car packages))
        (t (%package-named name (cdr packages)))))

(defun find-package (designator)
  (if (%package-p designator)
      designator
      (%package-named (%designator-name designator) %all-packages)))

;;; The package a designator names, or an error saying there is none.
(defun %the-package (designator)
  (let ((package (find-package designator)))
    (if package
        package
        (error "there is no package with that name"))))

(defun package-name (package) (%package-name (%the-package package)))
(defun package-nicknames (package) (%package-nicknames (%the-package package)))
(defun package-use-list (package) (%package-uses (%the-package package)))
(defun list-all-packages () (append %all-packages nil))

;;; ------------------------------------------------------------------------
;;; Making and using packages

(defun make-package (name &key nicknames use)
  (let ((name (%designator-name name)))
    (if (find-package name)
        (error "a package with that name already exists"))
    (let ((package (make-%package :name name
                                  :nicknames (%designator-names nicknames)
                                  :internals (make-hash-table :test 'equal)
                                  :externals (make-hash-table :test 'equal)
                                  :uses nil)))
      (setq %all-packages (cons package %all-packages))
      (use-package use package)
      package)))

;;; Make each package in PACKAGES used by PACKAGE: its external symbols become
;;; visible in PACKAGE without a prefix. Name conflicts are not yet checked —
;;; the useful core first, the rest on demand (#41).
(defun use-package (packages &optional (package *package*))
  (let ((user (%the-package package)))
    (%use-each (if (listp packages) packages (list packages)) user)
    t))

(defun %use-each (packages user)
  (if (endp packages)
      nil
      (let ((used (%the-package (car packages))))
        (if (not (member used (%package-uses user)))
            (setf (%package-uses user) (append (%package-uses user) (list used))))
        (%use-each (cdr packages) user))))

;;; ------------------------------------------------------------------------
;;; Symbols in packages

;;; Make PACKAGE the home of SYMBOL and file it under its name, as external or
;;; internal. The one place a symbol joins a package.
(defun %adopt (symbol package external)
  (%set-symbol-home symbol package)
  (setf (gethash (symbol-name symbol)
                 (if external (%package-externals package) (%package-internals package)))
        symbol)
  symbol)

;;; `(find-symbol name &optional package)` — the symbol of that name accessible
;;; in PACKAGE, and how: `:external`, `:internal` or `:inherited`. Two values,
;;; and `nil nil` if there is none.
(defun find-symbol (string &optional (package *package*))
  (let ((package (%the-package package)))
    (if (eq package %keyword-package)
        (let ((keyword (%find-system-symbol string t)))
          (if keyword (values keyword :external) (values nil nil)))
        (%find-in package string))))

(defun %find-in (package name)
  (multiple-value-bind (symbol present) (gethash name (%package-externals package))
    (if present
        (values symbol :external)
        (multiple-value-bind (symbol present) (gethash name (%package-internals package))
          (if present
              (values symbol :internal)
              (multiple-value-bind (symbol present) (%find-inherited name (%package-uses package))
                (if present
                    (values symbol :inherited)
                    (%find-unadopted package name))))))))

(defun %find-inherited (name packages)
  (if (endp packages)
      (values nil nil)
      (multiple-value-bind (symbol present) (gethash name (%package-externals (car packages)))
        (if present
            (values symbol t)
            (%find-inherited name (cdr packages))))))

;;; The gap until step C. The compiled world still reads with the Rust reader,
;;; which interns into the system table and knows nothing of `*package*`, so a
;;; symbol read since adoption is there, home `0`, in no package. Reading it
;;; in `COMMON-LISP-USER` is what the Lisp reader will do, so that is where it
;;; is adopted, the first time anything asks for it there.
(defun %find-unadopted (package name)
  (if (eq package %cl-user-package)
      (let ((symbol (%find-system-symbol name nil)))
        (if (and symbol (eq (%symbol-home symbol) 0))
            (values (%adopt symbol package nil) :internal)
            (values nil nil)))
      (values nil nil)))

;;; `(intern name &optional package)` — `find-symbol`, and if there is no such
;;; symbol, a new one, internal to PACKAGE. The second value is the status of
;;; the symbol found, or `nil` if it had to be made.
(defun intern (string &optional (package *package*))
  (let ((package (%the-package package)))
    (multiple-value-bind (symbol status) (find-symbol string package)
      (cond (status (values symbol status))
            ;; A keyword is made the Rust way, so that a keyword made by the
            ;; reader and one made here are one object.
            ((eq package %keyword-package) (values (%intern-keyword string) nil))
            ;; In `COMMON-LISP-USER`, a new symbol is made in the system table
            ;; too, for the same reason: the Rust reader, which the build and
            ;; some tests still use, interns there, and must find the symbol
            ;; this made — `defstruct`'s `point-x` (#62, part 2) — rather than
            ;; make a second one. The other direction is `%find-unadopted`.
            ((and (eq package %cl-user-package) (null (%find-system-symbol string nil)))
             (values (%adopt (%intern string) package nil) nil))
            (t (values (%adopt (make-symbol string) package nil) nil))))))

;;; Make SYMBOLS external in PACKAGE. An inherited symbol becomes present as
;;; it does; its home does not change.
(defun export (symbols &optional (package *package*))
  (let ((package (%the-package package)))
    (%export-each (if (listp symbols) symbols (list symbols)) package)
    t))

(defun %export-each (symbols package)
  (if (endp symbols)
      nil
      (let ((name (symbol-name (car symbols))))
        (remhash name (%package-internals package))
        (setf (gethash name (%package-externals package)) (car symbols))
        (%export-each (cdr symbols) package))))

;;; The package SYMBOL belongs to. Its home says, in one of four forms (see
;;; `symbol::HOME` in heap.rs): nil for none, 1 for a keyword, a package, or 0
;;; for a symbol the Rust reader made since adoption — which the gap rule
;;; above places in `COMMON-LISP-USER`.
(defun symbol-package (symbol)
  (let ((home (%symbol-home symbol)))
    (cond ((null home) nil)
          ((eq home 1) %keyword-package)
          ((eq home 0) (%adopt symbol %cl-user-package nil) %cl-user-package)
          (t home))))

;;; ------------------------------------------------------------------------
;;; The forms programs write

;;; `(in-package name)` — make NAME the current package. Until the Lisp reader
;;; exists (step C), this sets `*package*` but does not change how the Rust
;;; reader interns what it reads.
(defmacro in-package (name)
  `(setq *package* (%the-package ',name)))

;;; `(defpackage name option...)` with `:nicknames`, `:use`, `:export` and
;;; `:intern`. A package that exists is added to rather than replaced.
(defmacro defpackage (name &rest options)
  `(%define-package ',name ',options))

(defun %define-package (name options)
  (let ((package (or (find-package name) (make-package name))))
    (%apply-package-options package options)
    package))

(defun %apply-package-options (package options)
  (if (endp options)
      nil
      (let ((option (car options)))
        (cond ((eq (car option) :nicknames)
               (setf (%package-nicknames package)
                     (append (%package-nicknames package) (%designator-names (cdr option)))))
              ((eq (car option) :use) (use-package (cdr option) package))
              ((eq (car option) :export) (export (%intern-all (cdr option) package) package))
              ((eq (car option) :intern) (%intern-all (cdr option) package))
              (t (error "this defpackage option is not supported yet")))
        (%apply-package-options package (cdr options)))))

(defun %intern-all (designators package)
  (if (endp designators)
      nil
      (cons (intern (%designator-name (car designators)) package)
            (%intern-all (cdr designators) package))))

;;; ------------------------------------------------------------------------
;;; The standard packages, and adoption

(defvar %common-lisp-package (make-package "COMMON-LISP" :nicknames '("CL")))
(defvar %keyword-package (make-package "KEYWORD"))
(defvar %quokka-package (make-package "QUOKKA" :use '("COMMON-LISP")))
(defvar %cl-user-package
  (make-package "COMMON-LISP-USER" :nicknames '("CL-USER") :use '("COMMON-LISP")))

(defvar *package* %cl-user-package)

;;; Sort every symbol in the Rust system table into a package: each of the
;;; standard's 978 names into `COMMON-LISP`, as an external symbol — creating
;;; the ones nothing has used yet, such as `LOOP` — and everything else into
;;; `QUOKKA`, internally. Keywords are already `KEYWORD`'s, by their home.
(defun %adopt-system-symbols ()
  (%adopt-common-lisp %common-lisp-symbol-names)
  (%adopt-the-rest (%system-symbols))
  ;; The names have done their job — each symbol has its own copy now — and a
  ;; list of 978 strings is not worth carrying in every image.
  (setq %common-lisp-symbol-names nil)
  t)

(defun %adopt-common-lisp (names)
  (if (endp names)
      nil
      (progn (%adopt (%intern (car names)) %common-lisp-package t)
             (%adopt-common-lisp (cdr names)))))

(defun %adopt-the-rest (symbols)
  (if (endp symbols)
      nil
      (progn (if (eq (%symbol-home (car symbols)) 0)
                 (%adopt (car symbols) %quokka-package nil))
             (%adopt-the-rest (cdr symbols)))))

;;; `defstruct`'s derived names — `make-point`, `point-x` — in the package
;;; current when the `defstruct` is expanded, as CLHS says (#62, part 2). The
;;; prelude's version interns by name alone, which is all there was before
;;; packages, and such a symbol ends up in `CL-USER` whatever `*package*`
;;; is. Redefined here, where packages exist; calls are late-bound, so
;;; `defstruct` uses this one from now on. The compiler and the prelude use no
;;; `defstruct`, so the fixpoint does not see the change.
(defun %symbol-concat (a b)
  (values (intern (%string-append (symbol-name a) (symbol-name b)) *package*)))

;;; The last form of the library: from here on, every symbol has a package.
(%adopt-system-symbols)
