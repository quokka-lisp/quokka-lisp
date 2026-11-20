;;; Hash tables — the part the oracle can see.
;;;
;;; Writing an entry is `(setf (gethash k table) v)` in Common Lisp, and
;;; `setf` is tier 1. So the only way to put anything in a table is
;;; `%set-gethash`, a name SBCL does not have — which means **almost all of
;;; this feature is invisible to the differential tests**, exactly as vector
;;; mutation is. What is left is the part that needs no writing.
;;;
;;; `crates/quokka-vm/src/boot.rs` covers the rest with Rust tests, which are
;;; weaker: they check Quokka against expectations rather than against the
;;; standard. This file exists partly to record the gap.

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
