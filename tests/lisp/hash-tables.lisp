;;; Hash tables — the part the oracle can see.
;;;
;;; Keys are hashed by content where they have one, and otherwise by
;;; address, which the collector invalidates whenever it moves things, so a
;;; table holding such a key rehashes after a collection (#64).
;;;
;;; This file could not exist in any useful form until `setf` did: writing an
;;; entry has no name in Common Lisp other than `(setf (gethash k h) v)`.

(hash-table-p (make-hash-table))
(hash-table-p (make-hash-table :test 'equal))
(hash-table-p 1)
(hash-table-p nil)
(hash-table-p '(1 2))
(hash-table-p "x")

(hash-table-count (make-hash-table))
(hash-table-count (make-hash-table :test 'equal))

;; A key that was never stored, with and without a default.
(gethash 'missing (make-hash-table))
(gethash 'missing (make-hash-table) 'fallback)
(gethash 1 (make-hash-table) 'fallback)
(gethash "s" (make-hash-table :test 'equal) 'fallback)

;; Removing something that is not there says so.
(remhash 'missing (make-hash-table))

;; maphash over an empty table calls nothing and yields nil.
(maphash (lambda (k v) (list k v)) (make-hash-table))

;; Storing and finding, now that there is a standard way to store.
(let ((h (make-hash-table)))
  (setf (gethash 'a h) 1)
  (setf (gethash 'b h) 2)
  (list (gethash 'a h) (gethash 'b h) (gethash 'c h) (hash-table-count h)))

;; Storing twice under one key replaces rather than adds.
(let ((h (make-hash-table)))
  (setf (gethash 'a h) 1)
  (setf (gethash 'a h) 2)
  (list (gethash 'a h) (hash-table-count h)))

;; setf yields the value.
(let ((h (make-hash-table))) (setf (gethash 'a h) 'yielded))

;; Several at once.
(let ((h (make-hash-table)))
  (setf (gethash 'a h) 1 (gethash 'b h) 2)
  (list (gethash 'a h) (gethash 'b h)))

;; Fixnums, characters and symbols are all hashable by content.
(let ((h (make-hash-table))) (setf (gethash 1 h) 'one) (gethash 1 h))
(let ((h (make-hash-table))) (setf (gethash #\a h) 'char) (gethash #\a h))

;; A keyword and an ordinary symbol share a name and are different keys.
(let ((h (make-hash-table)))
  (setf (gethash :k h) 'keyword)
  (setf (gethash 'k h) 'symbol)
  (list (gethash :k h) (gethash 'k h)))

;; Strings match by content, in an equal table.
(let ((h (make-hash-table :test 'equal)))
  (setf (gethash "key" h) 'found)
  (list (gethash "key" h) (gethash "other" h 'no)))

;; Removing.
(let ((h (make-hash-table)))
  (setf (gethash 'a h) 1)
  (list (remhash 'a h) (gethash 'a h) (hash-table-count h)))

;; Growing past the initial bucket count must not lose or duplicate anything.
(let ((h (make-hash-table)))
  (labels ((fill-up (i)
             (if (= i 200) h (progn (setf (gethash i h) (* i i)) (fill-up (+ i 1))))))
    (let ((full (fill-up 0)))
      (list (hash-table-count full)
            (gethash 0 full)
            (gethash 7 full)
            (gethash 199 full)
            (gethash 200 full 'absent)))))

;; maphash visits every entry.
(let ((h (make-hash-table)) (total 0))
  (setf (gethash 'a h) 1)
  (setf (gethash 'b h) 2)
  (setf (gethash 'c h) 3)
  (maphash (lambda (k v) (setq total (+ total v))) h)
  total)
;; Any object is a key, as itself (#64): a list finds its own entry and not
;; that of an equal list; a vector likewise, in an equal table too.
(let ((h (make-hash-table)) (k (list 1 2)) (v (vector 1)))
  (setf (gethash k h) 'list)
  (setf (gethash v h) 'vector)
  (list (gethash k h) (gethash (list 1 2) h 'other) (gethash v h)))

(let ((h (make-hash-table :test 'equal)) (v (vector 1)))
  (setf (gethash v h) 'vector)
  (list (gethash v h) (gethash (vector 1) h 'other)))

;; Still found after the collector has moved every key: enough garbage to
;; collect several times between storing and finding.
(defun churn (n)
  (if (> n 0) (progn (make-string 64 :initial-element #\x) (churn (- n 1))) nil))

(let ((h (make-hash-table :test 'eq)) (a (list 'a)) (b (list 'b)) (c (vector 'c)))
  (setf (gethash a h) 1)
  (setf (gethash b h) 2)
  (setf (gethash c h) 3)
  (churn 20000)
  (remhash b h)
  (churn 20000)
  (list (gethash a h) (gethash b h 'gone) (gethash c h) (hash-table-count h)))

;; A string in an eql table is the same key only as itself, after collections too.
(let ((h (make-hash-table)) (s (copy-seq "ab")))
  (setf (gethash s h) 'found)
  (churn 20000)
  (list (gethash s h) (gethash (copy-seq "ab") h 'other)))
