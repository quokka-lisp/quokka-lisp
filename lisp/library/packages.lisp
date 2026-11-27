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
;;; the one that hashes strings by content. `uses` and `used-by` are the two
;;; directions of `use-package`, kept in step by the functions that change
;;; them (#66, decision 2); `shadowing` is the list of symbols that win a name
;;; conflict in this package (CLHS 11.1.1.2.5).
(defstruct %package name nicknames internals externals uses used-by shadowing)

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
  (cond ((stringp designator) (%simple-string designator))
        ((symbolp designator) (symbol-name designator))
        ((characterp designator) (string designator))
        (t (error 'type-error "a package or symbol name must be a string or a symbol"
                  designator))))

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
        (%package-error designator "there is no package with that name"))))

(defun package-name (package) (%package-name (%the-package package)))
(defun package-nicknames (package) (%package-nicknames (%the-package package)))
(defun package-use-list (package) (copy-list (%package-uses (%the-package package))))
(defun package-used-by-list (package) (copy-list (%package-used-by (%the-package package))))
(defun package-shadowing-symbols (package)
  (copy-list (%package-shadowing (%the-package package))))
(defun list-all-packages () (append %all-packages nil))

;;; A designator, or a list of them, as a list.
(defun %as-list (x) (if (listp x) x (list x)))

;;; Every package error says which package (CLHS `package-error`). Quokka's
;;; conditions are flat (#26): the type, and the message and package as data,
;;; until tier 2's `make-condition` gives them slots.
(defun %package-error (package message)
  (error 'package-error message package))

;;; ------------------------------------------------------------------------
;;; Making and using packages

;;; A name already taken is a correctable error, as in SBCL: its `continue`
;;; restart returns the package that has it (step B, #125).
(defun make-package (name &key nicknames use)
  (let ((name (%designator-name name))
        (nicknames (%designator-names nicknames)))
    (let ((taken (%package-with-a-name (cons name nicknames) nil)))
      (when taken
        (restart-case (%package-error taken "a package with that name already exists")
          (continue ()
            :report "Return the package that has the name."
            (return-from make-package taken)))))
    (let ((package (make-%package :name name
                                  :nicknames nicknames
                                  :internals (make-hash-table :test 'equal)
                                  :externals (make-hash-table :test 'equal)
                                  :uses nil :used-by nil :shadowing nil)))
      (setq %all-packages (cons package %all-packages))
      (use-package use package)
      package)))

;;; NAME and NICKNAMES name no package, other than PACKAGE itself.
(defun %check-names-free (name nicknames package)
  (let ((other (%package-with-a-name (cons name nicknames) package)))
    (if other (%package-error other "a package with that name already exists"))))

;;; A package other than PACKAGE that one of NAMES names, or nil.
(defun %package-with-a-name (names package)
  (dolist (each names nil)
    (let ((other (find-package each)))
      (if (and other (not (eq other package))) (return other)))))

;;; Make each package in PACKAGES used by PACKAGE: its external symbols become
;;; visible in PACKAGE without a prefix. Each is checked for name conflicts
;;; first (#66, decision 1): an external symbol of the used package must not
;;; meet a different symbol of the same name already accessible in PACKAGE,
;;; unless that one is shadowing. On a conflict nothing changes.
(defun use-package (packages &optional (package *package*))
  (let ((user (%the-package package)))
    (dolist (designator (%as-list packages))
      (let ((used (%the-package designator)))
        (if (eq used %keyword-package)
            (%package-error used "the KEYWORD package cannot be used"))
        (if (not (or (eq used user) (member used (%package-uses user))))
            (progn
              (dolist (symbol (%external-symbols used))
                (multiple-value-bind (found status) (find-symbol (symbol-name symbol) user)
                  (if (and status (not (eq found symbol))
                           (not (member found (%package-shadowing user))))
                      (%package-error user "using that package would make two symbols of one name accessible"))))
              (setf (%package-uses user) (append (%package-uses user) (list used)))
              (setf (%package-used-by used) (cons user (%package-used-by used)))))))
    t))

(defun unuse-package (packages &optional (package *package*))
  (let ((user (%the-package package)))
    (dolist (designator (%as-list packages))
      (let ((used (%the-package designator)))
        (setf (%package-uses user) (remove used (%package-uses user)))
        (setf (%package-used-by used) (remove user (%package-used-by used)))))
    t))

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
  (let ((package (%the-package package))
        (string (%simple-string string)))
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
  (let ((package (%the-package package))
        ;; A string with a fill pointer is copied: the name is kept, and
        ;; Rust's symbol table takes simple strings only (#67).
        (string (%simple-string string)))
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

;;; Make SYMBOLS external in PACKAGE. Each must be accessible there; an
;;; inherited one becomes present as it is exported, and its home does not
;;; change. Every package that uses PACKAGE is checked first: one that has a
;;; different symbol of the same name, not shadowing, would meet a conflict.
(defun export (symbols &optional (package *package*))
  (let ((package (%the-package package))
        (symbols (%as-list symbols)))
    (dolist (symbol symbols)
      (let ((name (symbol-name symbol)))
        (multiple-value-bind (found status) (find-symbol name package)
          (if (not (and status (eq found symbol)))
              (%package-error package "a symbol must be accessible in a package to be exported")))
        (dolist (user (%package-used-by package))
          (multiple-value-bind (found status) (find-symbol name user)
            (if (and status (not (eq found symbol))
                     (not (member found (%package-shadowing user))))
                (%package-error user "exporting that symbol would make two symbols of one name accessible"))))))
    (if (not (eq package %keyword-package))
        (dolist (symbol symbols)
          (let ((name (symbol-name symbol)))
            (remhash name (%package-internals package))
            (setf (gethash name (%package-externals package)) symbol))))
    t))

;;; Make SYMBOLS internal again. Each must be accessible in PACKAGE.
(defun unexport (symbols &optional (package *package*))
  (let ((package (%the-package package))
        (symbols (%as-list symbols)))
    (if (eq package %keyword-package)
        (%package-error package "a keyword cannot be unexported"))
    (dolist (symbol symbols)
      (multiple-value-bind (found status) (find-symbol (symbol-name symbol) package)
        (if (not (and status (eq found symbol)))
            (%package-error package "a symbol must be accessible in a package to be unexported"))))
    (dolist (symbol symbols)
      (let ((name (symbol-name symbol)))
        (if (eq (gethash name (%package-externals package)) symbol)
            (progn (remhash name (%package-externals package))
                   (setf (gethash name (%package-internals package)) symbol)))))
    t))

;;; ------------------------------------------------------------------------
;;; Present symbols: importing, shadowing, uninterning (step S, #66)
;;;
;;; A symbol is *present* in a package when the package's own tables hold it,
;;; and *accessible* when it is present or inherited from a used package. A
;;; name conflict is two different symbols of one name both accessible; the
;;; package's shadowing symbols are the ones declared to win.

;;; The symbol of NAME present in PACKAGE, and `:external` or `:internal`.
(defun %present (package name)
  (multiple-value-bind (symbol found) (gethash name (%package-externals package))
    (if found
        (values symbol :external)
        (multiple-value-bind (symbol found) (gethash name (%package-internals package))
          (if found (values symbol :internal) (values nil nil))))))

;;; Make SYMBOL present in PACKAGE as an internal symbol, and give it a home
;;; if it has none.
(defun %make-present (symbol package)
  (if (null (nth-value 1 (%present package (symbol-name symbol))))
      (setf (gethash (symbol-name symbol) (%package-internals package)) symbol))
  (if (null (symbol-package symbol)) (%set-symbol-home symbol package))
  symbol)

;;; Take whatever symbol of NAME is present out of PACKAGE's tables.
(defun %remove-present (package name)
  (multiple-value-bind (symbol status) (%present package name)
    (if status
        (progn
          (remhash name (%package-externals package))
          (remhash name (%package-internals package))
          (setf (%package-shadowing package) (remove symbol (%package-shadowing package)))
          (if (eq (%symbol-home symbol) package) (%set-symbol-home symbol nil))))
    status))

;;; `(import symbols [package])` — make each present in PACKAGE. A different
;;; symbol of the same name already accessible is a conflict, inherited or
;;; not, and so are two of one name among SYMBOLS.
(defun import (symbols &optional (package *package*))
  (let ((package (%the-package package))
        (symbols (%as-list symbols)))
    (dolist (symbol symbols)
      (multiple-value-bind (found status) (find-symbol (symbol-name symbol) package)
        (if (and status (not (eq found symbol)))
            (%package-error package "importing that symbol would make two symbols of one name accessible")))
      (dolist (other symbols)
        (if (and (not (eq other symbol)) (string= (symbol-name other) (symbol-name symbol)))
            (%package-error package "two symbols of one name cannot be imported together"))))
    (dolist (symbol symbols) (%make-present symbol package))
    t))

;;; `(shadowing-import symbols [package])` — make each present in PACKAGE and
;;; a shadowing symbol, displacing a different present symbol of the same
;;; name. Never a conflict: that is what it is for.
(defun shadowing-import (symbols &optional (package *package*))
  (let ((package (%the-package package)))
    (dolist (symbol (%as-list symbols))
      (let ((name (symbol-name symbol)))
        (multiple-value-bind (found status) (%present package name)
          (if (and status (not (eq found symbol))) (%remove-present package name)))
        (%make-present symbol package)
        (if (not (member symbol (%package-shadowing package)))
            (setf (%package-shadowing package) (cons symbol (%package-shadowing package))))))
    t))

;;; `(shadow names [package])` — for each name, the symbol present in PACKAGE
;;; becomes a shadowing symbol, made first if there is none: the way a
;;; package declares in advance that its `FOO` is not the `FOO` it inherits.
(defun shadow (names &optional (package *package*))
  (let ((package (%the-package package)))
    (dolist (designator (%as-list names))
      (let ((name (%designator-name designator)))
        (let ((symbol (multiple-value-bind (found status) (%present package name)
                        (if status found (%adopt (make-symbol name) package nil)))))
          (if (not (member symbol (%package-shadowing package)))
              (setf (%package-shadowing package) (cons symbol (%package-shadowing package)))))))
    t))

;;; `(unintern symbol [package])` — take SYMBOL out of PACKAGE, if present,
;;; and true if it was. Removing a shadowing symbol may expose two different
;;; inherited symbols of its name: a conflict, and nothing is removed.
(defun unintern (symbol &optional (package *package*))
  (let* ((package (%the-package package))
         (name (symbol-name symbol)))
    (multiple-value-bind (found status) (%present package name)
      (if (and status (eq found symbol))
          (progn
            (if (member symbol (%package-shadowing package))
                (let ((inherited nil))
                  (dolist (used (%package-uses package))
                    (multiple-value-bind (other external) (gethash name (%package-externals used))
                      (if (and external (not (member other inherited)))
                          (setq inherited (cons other inherited)))))
                  (if (cdr inherited)
                      (%package-error package "uninterning that symbol would make two symbols of one name accessible"))))
            (%remove-present package name)
            t)
          nil))))

;;; ------------------------------------------------------------------------
;;; Deleting and renaming (step S, #66)

;;; `(delete-package package)` — as SBCL does it (decision 3): a package still
;;; used by another is an error; otherwise it stops using its own, leaves the
;;; list of packages, its symbols lose their home, and the object is left
;;; with no name. Deleting it again answers nil. The packages the system
;;; itself lives in are refused.
(defun delete-package (designator)
  (let ((package (if (%package-p designator) designator (find-package designator))))
    ;; No such package is a correctable error (CLHS): continued, nothing
    ;; is deleted and the answer is nil.
    (cond ((null package)
           (restart-case (%package-error designator "there is no package with that name")
             (continue () :report "Return nil, deleting nothing." nil)))
          ((null (%package-name package)) nil)
          ((member package (list %common-lisp-package %keyword-package %quokka-package))
           (%package-error package "a package the system needs cannot be deleted"))
          ;; A correctable error (CLHS): continued, the package is no
          ;; longer used by those that used it, and then deleted.
          ((%package-used-by package)
           (restart-case (%package-error package "a package used by another cannot be deleted")
             (continue ()
               :report "Stop the packages that use it from using it, and delete it."
               (dolist (user (%package-used-by package)) (unuse-package package user))
               (delete-package package))))
          (t (unuse-package (%package-uses package) package)
             (setq %all-packages (remove package %all-packages))
             (dolist (symbol (%present-symbols package))
               (if (eq (%symbol-home symbol) package) (%set-symbol-home symbol nil)))
             (setf (%package-name package) nil)
             (setf (%package-nicknames package) nil)
             t))))

;;; `(rename-package package new-name [new-nicknames])` — the old name and
;;; nicknames are all replaced. The new ones must not name another package.
(defun rename-package (package new-name &optional new-nicknames)
  (let ((package (%the-package package))
        (name (%designator-name new-name))
        (nicknames (%designator-names new-nicknames)))
    (%check-names-free name nicknames package)
    (setf (%package-name package) name)
    (setf (%package-nicknames package) nicknames)
    package))

;;; ------------------------------------------------------------------------
;;; Walking a package's symbols (step S, #66)
;;;
;;; Every way of iterating takes a list of the symbols first and walks that
;;; (decision 4): changing the package from the body then cannot disturb the
;;; walk, which CLHS leaves undefined anyway. The keyword package keeps its
;;; symbols in the Rust system table (their home is 1), so it is listed from
;;; there.

(defun %table-values (table)
  (let ((values nil))
    (maphash (lambda (key value) key (setq values (cons value values))) table)
    values))

(defun %keywords () (remove-if-not #'keywordp (%system-symbols)))

(defun %external-symbols (package)
  (if (eq package %keyword-package)
      (%keywords)
      (%table-values (%package-externals package))))

(defun %internal-symbols (package)
  (if (eq package %keyword-package) nil (%table-values (%package-internals package))))

(defun %present-symbols (package)
  (append (%external-symbols package) (%internal-symbols package)))

;;; The symbols PACKAGE inherits: each used package's externals that are not
;;; shadowed by a present symbol of the same name, each once. Two used
;;; packages may export one symbol; an `eq` table, not `member`, finds the
;;; repeat, since a package using `COMMON-LISP` inherits 978 symbols and a
;;; list search would make that quadratic.
(defun %inherited-symbols (package)
  (let ((inherited nil)
        (seen (make-hash-table :test 'eq)))
    (dolist (used (%package-uses package))
      (dolist (symbol (%external-symbols used))
        (if (and (null (nth-value 1 (%present package (symbol-name symbol))))
                 (not (gethash symbol seen)))
            (progn (setf (gethash symbol seen) t)
                   (setq inherited (cons symbol inherited))))))
    inherited))

(defun %accessible-symbols (package)
  (let ((package (%the-package package)))
    (append (%present-symbols package) (%inherited-symbols package))))

;;; `(do-symbols (var [package [result]]) body...)` — BODY for every symbol
;;; accessible in PACKAGE; `do-external-symbols` for its external ones;
;;; `do-all-symbols` for every symbol present in any package, which may
;;; visit a symbol more than once, as CLHS allows.
(defmacro do-symbols ((var &optional (package '*package*) result) &body body)
  `(dolist (,var (%accessible-symbols ,package) ,result) ,@body))

(defmacro do-external-symbols ((var &optional (package '*package*) result) &body body)
  `(dolist (,var (%external-symbols (%the-package ,package)) ,result) ,@body))

(defmacro do-all-symbols ((var &optional result) &body body)
  `(dolist (,var (%all-present-symbols) ,result) ,@body))

(defun %all-present-symbols ()
  (let ((all (%keywords)))
    (dolist (package %all-packages)
      (if (not (eq package %keyword-package))
          (setq all (append (%present-symbols package) all))))
    all))

;;; `(find-all-symbols name)` — every symbol of that name present in any
;;; package, each once.
(defun find-all-symbols (name)
  (let ((name (%designator-name name))
        (found nil))
    (dolist (package %all-packages)
      (multiple-value-bind (symbol status)
          (if (eq package %keyword-package) (find-symbol name package) (%present package name))
        (if (and status (not (member symbol found)))
            (setq found (cons symbol found)))))
    found))

;;; `(with-package-iterator (name packages type...) body...)` — within BODY,
;;; `(name)` returns the next symbol of PACKAGES whose status is one of
;;; TYPEs, as four values: true, the symbol, its status and the package; and
;;; nil when there are no more. A local macro over a list made at the start.
(defmacro with-package-iterator ((name packages &rest types) &body body)
  (if (null types) (error 'program-error "with-package-iterator needs a symbol type"))
  (dolist (type types)
    (if (not (member type '(:internal :external :inherited)))
        (error 'program-error "with-package-iterator's types are :internal, :external and :inherited")))
  (let ((entries (gensym "ENTRIES")))
    `(let ((,entries (%iterator-entries ,packages ',types)))
       (macrolet ((,name () (list '%iterator-step ',entries)))
         ,@body))))

(defun %iterator-entries (packages types)
  (let ((entries nil))
    (dolist (designator (%as-list packages))
      (let ((package (%the-package designator)))
        (if (member :internal types)
            (dolist (symbol (%internal-symbols package))
              (setq entries (cons (list symbol :internal package) entries))))
        (if (member :external types)
            (dolist (symbol (%external-symbols package))
              (setq entries (cons (list symbol :external package) entries))))
        (if (member :inherited types)
            (dolist (symbol (%inherited-symbols package))
              (setq entries (cons (list symbol :inherited package) entries))))))
    entries))

(defmacro %iterator-step (entries)
  (let ((entry (gensym "ENTRY")))
    `(let ((,entry (car ,entries)))
       (if ,entry
           (progn (setq ,entries (cdr ,entries))
                  (values t (first ,entry) (second ,entry) (third ,entry)))
           nil))))

;;; `(gensym [x])` — a new uninterned symbol, named "G" and the value of
;;; `*gensym-counter*`, which advances; or named X and the counter, when X is
;;; a string; or "G" and X, without advancing, when X is an integer (step Y,
;;; #72). Until here `gensym` was the primitive, whose counter is in Rust and
;;; which ignored its argument: CLHS makes the counter a special variable a
;;; program may bind, so the counter has to be one, and then `gensym` has to
;;; read it. Here because this file loads last, when the digits and `coerce`
;;; it needs exist; a macro in an earlier file still gets the primitive. The
;;; variable is in `symbols.lisp`, so that `rebuild.lisp` may bind it.
;;;
;;; The compiler calls this `gensym` when it compiles itself. A name now
;;; carries its prefix — `SETF-EXPANDER1000` — so its length depends on the
;;; counter, and a longer name is a longer string; a rebuild therefore
;;; counts from zero (`%rebuild-world`), or two rebuilds would differ in
;;; size for no reason but their numbering.
;;; INTEGER's decimal digits, for a name. Two loops over the number — count
;;; the digits, then fill a string from its end — and nothing allocated but
;;; the string. It was `(coerce (mapcar #'digit-char (%integer-digits ...))
;;; 'string)`, which is the same thing said in one line and cost some 6,800
;;; steps: a list of digits, a list of characters, and `coerce` finding out
;;; what a string is. Every label the compiler makes and every temporary a
;;; `setf` expansion needs is a `gensym`, so that was a quarter of a rebuild
;;; (#105). `%quotient` and `rem` are primitives; a bignum falls back to the
;;; generic ones, as anywhere.
(defun %decimal-string (integer)
  (let ((length 1)
        (rest (%quotient integer 10)))
    (tagbody
     count
       (if (> rest 0)
           (progn (setq length (+ length 1))
                  (setq rest (%quotient rest 10))
                  (go count))))
    (let ((string (%make-string length #\0))
          (rest integer)
          (at length))
      (tagbody
       fill
         (setq at (- at 1))
         (%set-aref string at (code-char (+ 48 (rem rest 10))))
         (setq rest (%quotient rest 10))
         (if (> at 0) (go fill)))
      string)))

;;; Before `gensym`, which it serves: the forms below this point are compiled
;;; with the new `gensym`, and their macros call it.
(defun gensym (&optional (x "G"))
  (if (not (and (integerp *gensym-counter*) (>= *gensym-counter* 0)))
      (error 'type-error "*gensym-counter* wants a non-negative integer" *gensym-counter*))
  (let ((name (cond ((stringp x)
                     (prog1 (%string-append x (%decimal-string *gensym-counter*))
                       (setq *gensym-counter* (+ *gensym-counter* 1))))
                    ((and (integerp x) (>= x 0)) (%string-append "G" (%decimal-string x)))
                    (t (error 'type-error "gensym wants a string or a non-negative integer" x)))))
    (make-symbol name)))

;;; `(gentemp [prefix [package]])` — a new symbol interned in PACKAGE, named
;;; PREFIX and a counter, the counter advanced past any name already taken.
(defvar %gentemp-counter 0)

(defun gentemp (&optional (prefix "T") (package *package*))
  (setq %gentemp-counter (+ %gentemp-counter 1))
  (let ((name (%string-append prefix (princ-to-string %gentemp-counter))))
    (if (nth-value 1 (find-symbol name package))
        (gentemp prefix package)
        (values (intern name package)))))

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

;;; `(defpackage name option...)`. A package that exists is added to rather
;;; than replaced. The options are applied in the order CLHS gives, whatever
;;; order they are written in: shadowing first, so that what follows meets no
;;; conflict it was meant to avoid; then using, importing and interning; and
;;; exporting last. `:documentation` and `:size` are accepted and ignored.
(defmacro defpackage (name &rest options)
  `(%define-package ',name ',options))

;;; CLHS `defpackage` refuses, with a `program-error`, an option given twice
;;; that may be given once, and a symbol name that two options would each
;;; put in the package their own way (#79): one name in more than one of
;;; `:shadow`, `:shadowing-import-from`, `:import-from` and `:intern`, or in
;;; both `:intern` and `:export`. Checked before anything is made.
(defun %check-defpackage-options (options)
  (dolist (once '(:size :documentation))
    (if (> (count once options :key #'car) 1)
        (error 'program-error "this defpackage option may be given once" once)))
  (let ((placed nil)
        (interned nil)
        (exported nil))
    (dolist (option options)
      (let ((names (cond ((member (car option) '(:shadow :intern)) (cdr option))
                         ((member (car option) '(:shadowing-import-from :import-from))
                          (cddr option))
                         (t nil))))
        (dolist (name (mapcar #'%designator-name names))
          (if (member name placed :test #'string=)
              (error 'program-error "defpackage names a symbol in two options" name))
          (push name placed)
          (if (eq (car option) :intern) (push name interned))))
      (if (eq (car option) :export)
          (setq exported (append (mapcar #'%designator-name (cdr option)) exported))))
    (dolist (name interned)
      (if (member name exported :test #'string=)
          (error 'program-error "defpackage both interns and exports a name" name)))))

(defun %define-package (name options)
  (%check-defpackage-options options)
  (let ((package (or (find-package name) (make-package name))))
    (dolist (option options)
      (if (not (member (car option) '(:nicknames :use :export :intern :shadow
                                      :shadowing-import-from :import-from
                                      :documentation :size)))
          (%package-error package "this defpackage option is not known")))
    (dolist (option options)
      (cond ((eq (car option) :nicknames)
             (let ((nicknames (%designator-names (cdr option))))
               (%check-names-free (%package-name package) nicknames package)
               (setf (%package-nicknames package)
                     (append (%package-nicknames package) nicknames))))
            ((eq (car option) :shadow) (shadow (cdr option) package))
            ((eq (car option) :shadowing-import-from)
             (shadowing-import (%symbols-from (second option) (cddr option)) package))))
    (dolist (option options)
      (if (eq (car option) :use) (use-package (cdr option) package)))
    (dolist (option options)
      (cond ((eq (car option) :import-from)
             (import (%symbols-from (second option) (cddr option)) package))
            ((eq (car option) :intern) (%intern-all (cdr option) package))))
    (dolist (option options)
      (if (eq (car option) :export) (export (%intern-all (cdr option) package) package)))
    package))

;;; The symbols named NAMES accessible in package FROM; each must exist. One
;;; that does not is a correctable error, as in SBCL: its `continue`
;;; restart leaves the name out.
(defun %symbols-from (from names)
  (let ((package (%the-package from)) (found nil))
    (dolist (designator names (nreverse found))
      (multiple-value-bind (symbol status) (find-symbol (%designator-name designator) package)
        (if status
            (push symbol found)
            (restart-case (%package-error package "defpackage names a symbol that does not exist")
              (continue () :report "Leave the name out." nil)))))))

(defun %intern-all (designators package)
  (if (endp designators)
      nil
      (cons (intern (%designator-name (car designators)) package)
            (%intern-all (cdr designators) package))))

;;; ------------------------------------------------------------------------
;;; The standard packages, and adoption

(defvar %common-lisp-package (make-package "COMMON-LISP" :nicknames '("CL")))
(defvar %keyword-package (make-package "KEYWORD"))
;;; The metaobject protocol's names (`mop.lisp`), which are not Common
;;; Lisp's, have a package of their own, as SBCL's are in `SB-MOP`; `QUOKKA`
;;; uses it, so the library reads them without a prefix.
(defvar %quokka-mop-package (make-package "QUOKKA-MOP" :use '("COMMON-LISP")))
;;; The debugger's names (`frames.lisp`), likewise, as SBCL's are in
;;; `SB-DEBUG` (#130).
(defvar %quokka-debug-package (make-package "QUOKKA-DEBUG" :use '("COMMON-LISP")))
(defvar %quokka-package
  (make-package "QUOKKA" :use '("COMMON-LISP" "QUOKKA-MOP" "QUOKKA-DEBUG")))
(defvar %cl-user-package
  (make-package "COMMON-LISP-USER" :nicknames '("CL-USER") :use '("COMMON-LISP")))

(defvar *package* %cl-user-package)

;;; The names `QUOKKA-MOP` exports: the metaobject classes and the readers
;;; of `mop.lisp`, those of SBCL's `SB-MOP` that Quokka has.
(defvar %quokka-mop-symbol-names
  '("METAOBJECT" "SPECIALIZER" "EQL-SPECIALIZER" "FUNCALLABLE-STANDARD-CLASS"
    "FUNCALLABLE-STANDARD-OBJECT" "FORWARD-REFERENCED-CLASS" "SLOT-DEFINITION"
    "STANDARD-SLOT-DEFINITION" "DIRECT-SLOT-DEFINITION" "EFFECTIVE-SLOT-DEFINITION"
    "STANDARD-DIRECT-SLOT-DEFINITION" "STANDARD-EFFECTIVE-SLOT-DEFINITION"
    "STANDARD-ACCESSOR-METHOD" "STANDARD-READER-METHOD" "STANDARD-WRITER-METHOD"
    "CLASS-DIRECT-SUPERCLASSES" "CLASS-DIRECT-SUBCLASSES" "CLASS-PRECEDENCE-LIST"
    "CLASS-DIRECT-SLOTS" "CLASS-SLOTS" "CLASS-DIRECT-DEFAULT-INITARGS" "CLASS-DEFAULT-INITARGS"
    "CLASS-FINALIZED-P" "CLASS-PROTOTYPE" "FINALIZE-INHERITANCE" "ENSURE-CLASS"
    "SLOT-DEFINITION-NAME" "SLOT-DEFINITION-INITARGS" "SLOT-DEFINITION-INITFORM"
    "SLOT-DEFINITION-INITFUNCTION" "SLOT-DEFINITION-ALLOCATION" "SLOT-DEFINITION-TYPE"
    "SLOT-DEFINITION-READERS" "SLOT-DEFINITION-WRITERS" "SLOT-DEFINITION-LOCATION"
    "STANDARD-INSTANCE-ACCESS" "FUNCALLABLE-STANDARD-INSTANCE-ACCESS"
    "GENERIC-FUNCTION-NAME" "GENERIC-FUNCTION-METHODS" "GENERIC-FUNCTION-LAMBDA-LIST"
    "GENERIC-FUNCTION-METHOD-COMBINATION" "GENERIC-FUNCTION-METHOD-CLASS"
    "GENERIC-FUNCTION-DECLARATIONS" "GENERIC-FUNCTION-ARGUMENT-PRECEDENCE-ORDER"
    "METHOD-SPECIALIZERS" "METHOD-LAMBDA-LIST" "METHOD-GENERIC-FUNCTION" "METHOD-FUNCTION"
    "ACCESSOR-METHOD-SLOT-DEFINITION" "EQL-SPECIALIZER-OBJECT" "INTERN-EQL-SPECIALIZER"
    "SPECIALIZER-DIRECT-METHODS" "SPECIALIZER-DIRECT-GENERIC-FUNCTIONS"
    "FIND-METHOD-COMBINATION" "EXTRACT-LAMBDA-LIST" "EXTRACT-SPECIALIZER-NAMES"
    "SET-FUNCALLABLE-INSTANCE-FUNCTION"))

;;; The names `QUOKKA-DEBUG` exports: the frames of `frames.lisp`.
(defvar %quokka-debug-symbol-names
  '("FRAME" "FRAME-P" "FRAMES" "FRAME-FUNCTION" "FRAME-ARGUMENTS" "FRAME-NAME"
    "FRAME-CALL" "FRAME-LOCALS" "LIST-BACKTRACE" "PRINT-BACKTRACE" "UNWIND-TO-FRAME-AND-CALL"
    "RESTART-FRAME" "RETURN-FROM-FRAME" "ENABLE-DEBUGGER" "DISABLE-DEBUGGER"))

;;; Sort every symbol in the Rust system table into a package: each of the
;;; standard's 978 names into `COMMON-LISP`, as an external symbol — creating
;;; the ones nothing has used yet, such as `LOOP` — and everything else into
;;; `QUOKKA`, internally. Keywords are already `KEYWORD`'s, by their home.
(defun %adopt-system-symbols ()
  (%adopt-common-lisp %common-lisp-symbol-names)
  (dolist (name %quokka-mop-symbol-names)
    (%adopt (%intern name) %quokka-mop-package t))
  (dolist (name %quokka-debug-symbol-names)
    (%adopt (%intern name) %quokka-debug-package t))
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
