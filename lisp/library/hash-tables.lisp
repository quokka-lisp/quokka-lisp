;;;; Hash tables completed (step X, #71).
;;;;
;;;; The prelude's hash tables (chapter 19) take `eq`, `eql` and `equal` as
;;;; symbols, and an `equal` table refused a cons as a key. This file adds
;;;; the rest of CLHS 18: `equalp` tables, conses in `equal` ones, a test
;;;; given as a function, `sxhash`, the size and rehash parameters, and
;;;; `with-hash-table-iterator`.
;;;;
;;;; Decided in #71 (Carsten): Rust hashes a key by its contents, a bounded
;;;; way down (`hash::content`, which is also `sxhash`), and the library
;;;; compares. A cons in an `equal` table, or any key of an `equalp` one, is
;;;; the same key as another only by `equal` or `equalp`, and those are
;;;; defined once, in `lists.lisp`. So the primitives `%gethash`,
;;;; `%set-gethash` and `remhash` refuse such a key, and the machine hands
;;;; the call, arguments and all, to the functions below, the way a fixnum
;;;; operation that overflows hands its call to `%generic-+` (chapter 47).
;;;; Every other key never leaves Rust.

;;; The entry for KEY in TABLE, found in its bucket by the table's test.
(defun %hash-entry (key table)
  (let ((test (if (eq (hash-table-test table) 'equalp) #'equalp #'equal)))
    (dolist (entry (%hash-bucket key table) nil)
      (if (funcall test (car entry) key) (return entry)))))

(defun %generic-gethash (key table default)
  (let ((entry (%hash-entry key table)))
    (if entry (cdr entry) default)))

(defun %generic-set-gethash (key table value)
  (let ((entry (%hash-entry key table)))
    (if entry (setf (cdr entry) value) (%hash-insert key table value))
    value))

(defun %generic-remhash (key table)
  (let ((entry (%hash-entry key table)))
    (if entry (progn (%hash-delete-entry entry table) t) nil)))

;;; `:test` names a test or is one of the four functions: `#'equal` is
;;; `(symbol-function 'equal)`, so comparing the function objects tells
;;; which. `:size`, `:rehash-size` and `:rehash-threshold` are hints CLHS
;;; lets an implementation ignore; Quokka's tables start small and double.
(defun make-hash-table (&key (test 'eql) size rehash-size rehash-threshold)
  (%make-hash-table
   (cond ((member test '(eq eql equal equalp)) test)
         ((find test '(eq eql equal equalp) :key #'symbol-function))
         (t (error 'type-error "a hash table test: eq, eql, equal or equalp" test)))))

;;; What a table can hold before it grows. A table grows when it holds more
;;; than one entry a bucket, so its size is its bucket count, which Rust
;;; does not report; it starts at eight and is never below the count.
(defun hash-table-size (table)
  (max 8 (hash-table-count table)))

;;; A table doubles when it grows, and grows at one entry a bucket. Written
;;; through `%parse-double`, since no float literal may appear in the
;;; library (chapter 48).
(defvar %rehash-size (%parse-double "2.0"))
(defvar %rehash-threshold (%parse-double "1.0"))

(defun hash-table-rehash-size (table)
  (hash-table-test table)
  %rehash-size)

(defun hash-table-rehash-threshold (table)
  (hash-table-test table)
  %rehash-threshold)

;;; `(with-hash-table-iterator (next table) body...)`: in BODY, `(next)`
;;; returns three values, true with a key and its value while there are
;;; entries, and nil once there are none. The entries are taken when the
;;; form starts, so adding or removing while iterating changes nothing it
;;; sees (CLHS leaves that undefined).
(defmacro with-hash-table-iterator (spec &body body)
  (let ((box (gensym "ENTRIES")))
    `(let ((,box (list (%hash-table-entries ,(cadr spec)))))
       (macrolet ((,(car spec) () '(%hash-iterator-next ,box)))
         ,@body))))

(defun %hash-iterator-next (box)
  (let ((entries (car box)))
    (if entries
        (progn (setf (car box) (cdr entries))
               (values t (caar entries) (cdar entries)))
        nil)))
