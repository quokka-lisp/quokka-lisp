;;; Step X (#71): hash tables completed, and the standard streams. Tier 1:
;;; compiled and SBCL only. No table is printed, and nothing depends on the
;;; order maphash visits entries in or on what sxhash returns, which are
;;; each implementation's own.

;; equalp tables: case-insensitive strings, numbers by =, conses and
;; vectors by their elements.
(let ((table (make-hash-table :test 'equalp)))
  (setf (gethash "Apple" table) 1
        (gethash 2 table) :two
        (gethash '(a "b" 3) table) :list
        (gethash #(1 #\x) table) :vector)
  (list (gethash "APPLE" table) (gethash "apple" table)
        (gethash 2.0d0 table) (gethash '(a "B" 3.0d0) table)
        (gethash (vector 1.0d0 #\X) table) (gethash "pear" table)
        (hash-table-count table) (hash-table-test table)))

;; Replacing and removing in an equalp table.
(let ((table (make-hash-table :test #'equalp)))
  (setf (gethash "key" table) 1)
  (setf (gethash "KEY" table) 2)
  (list (hash-table-count table) (gethash "Key" table)
        (remhash "kEy" table) (remhash "key" table) (hash-table-count table)))

;; Conses in an equal table, compared by equal: case matters, and so does
;; the kind of number.
(let ((table (make-hash-table :test 'equal)))
  (setf (gethash '(1 2) table) :a
        (gethash '(x (y . z)) table) :b
        (gethash "s" table) :c)
  (list (gethash (list 1 2) table) (gethash '(x (y . z)) table)
        (multiple-value-list (gethash '(1 2.0d0) table))
        (gethash "s" table) (remhash (list 1 2) table) (gethash '(1 2) table)
        (hash-table-count table)))

;; Many keys, so the table grows and rehashes its content-hashed keys.
(let ((table (make-hash-table :test 'equal)))
  (dotimes (i 200) (setf (gethash (list i (* i i)) table) i))
  (let ((sum 0))
    (dotimes (i 200) (setq sum (+ sum (gethash (list i (* i i)) table))))
    (list (hash-table-count table) sum)))
(let ((table (make-hash-table :test 'equalp)))
  (dotimes (i 100) (setf (gethash (format nil "Key~D" i) table) i))
  (list (hash-table-count table) (gethash "KEY42" table) (gethash "key99" table)))

;; A test given as a function, or as a symbol.
(mapcar (lambda (test) (hash-table-test (make-hash-table :test test)))
        (list 'eq 'eql 'equal 'equalp #'eq #'eql #'equal #'equalp))

;; sxhash: equal objects hash alike, and the hash is a non-negative fixnum.
(list (= (sxhash (list 1 "ab" #\c)) (sxhash (list 1 (copy-seq "ab") #\c)))
      (= (sxhash "abc") (sxhash (copy-seq "abc")))
      (= (sxhash 'foo) (sxhash 'foo))
      (<= 0 (sxhash (list 1 2 3)) most-positive-fixnum)
      (>= (sxhash -5) 0))

;; Size and rehash parameters are accepted and reported.
(let ((table (make-hash-table :size 100 :rehash-size 2.0d0 :rehash-threshold 0.5d0)))
  (list (integerp (hash-table-size table))
        (>= (hash-table-size table) 0)
        (> (hash-table-rehash-size table) 1)
        (<= 0 (hash-table-rehash-threshold table) 1)))

;; with-hash-table-iterator visits every entry once.
(let ((table (make-hash-table)))
  (dotimes (i 5) (setf (gethash i table) (* 10 i)))
  (let ((sum 0) (entries 0))
    (with-hash-table-iterator (next table)
      (do () (nil)
        (multiple-value-bind (more key value) (next)
          (if (not more) (return))
          (setq sum (+ sum key value) entries (+ entries 1)))))
    (list entries sum)))
(with-hash-table-iterator (next (make-hash-table)) (multiple-value-list (next)))

;; clrhash and maphash on an equalp table.
(let ((table (make-hash-table :test 'equalp)) (total 0))
  (setf (gethash "a" table) 1 (gethash "B" table) 2)
  (maphash (lambda (key value) key (setq total (+ total value))) table)
  (clrhash table)
  (list total (hash-table-count table) (gethash "A" table)))

;; The standard streams exist and are output streams.
(mapcar #'output-stream-p (list *terminal-io* *debug-io* *query-io* *error-output* *trace-output*))
(with-output-to-string (*standard-output*) (format t "to standard output"))

;; setf of gethash evaluates a default form, in order (CLHS 5.1.1.1).
(let ((table (make-hash-table)) (order nil))
  (list (setf (gethash (progn (push 'key order) 'x)
                       (progn (push 'table order) table)
                       (progn (push 'default order) 0))
              (progn (push 'value order) 'y))
        (reverse order) (gethash 'x table)))
