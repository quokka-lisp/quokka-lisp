;;; The last plain failures and timeouts ansi-test found in tier 1 (step AH,
;;; #81): ~{~} taking its control from the arguments, a too-deep array,
;;; shifts past every bit, nthcdr and elt past the end of a list, handlers
;;; that name a condition's supertype, remove-duplicates through a hash
;;; table. Tier 1: compiled and SBCL only.

;; ~{~} with an empty body uses an argument as its control.
(format nil "~{~}" "~A-" '(1 2 3))
(format nil "~{~}" (formatter "~A") '(1 2 3 4))
(format nil "~3{~}" (formatter "~A") '(1 2 3 4))
(format nil "~:{~}" (formatter "~A") '((1 2) (3) (4 5 6)))
(format nil "~:{~}" "<~A ~A>" '((1 2) (3 4)))
(format nil "~@{~}" "~A." 1 2 3)

;; A nested array too deep to print is #.
(write-to-string #2A((#2A((a)))) :level 2 :pretty nil)
(write-to-string #2A((#2A((a)))) :level 3 :pretty nil)
(write-to-string #(1 #(2 #(3))) :level 2 :pretty nil)

;; ash and logbitp far past every bit.
(ash 1 -1000000)
(ash -1 -1000000)
(ash 0 1000000)
(logbitp most-positive-fixnum 0)
(logbitp most-positive-fixnum -1)
(logbitp 1000 (ash 1 1000))
(ash -5 -1)

;; nthcdr and elt past the end of a list.
(nthcdr 1000000 nil)
(nthcdr 5 '(a b))
(handler-case (elt '(a b) 1000000) (type-error () :type-error))

;; A handler names a supertype of the condition signalled.
(handler-case (read-from-string "") (stream-error () :stream-error))
(handler-case (read-from-string "(") (end-of-file () :end-of-file))
(handler-case (/ 1 0) (arithmetic-error () :arithmetic-error))
(handler-case (symbol-value (gensym)) (cell-error () :cell-error))
(handler-case (car 1) (program-error () :program-error) (error () :error))
(handler-case (error "x") (simple-condition () :simple-condition))
(handler-case (error "x") (serious-condition () :serious-condition))

;; remove-duplicates: the last of equal elements is kept, or with
;; from-end the first; with a standard test or a custom one.
(remove-duplicates '(a b a c b d))
(remove-duplicates '(a b a c b d) :from-end t)
(remove-duplicates '("a" "A" "b" "a") :test #'equal)
(remove-duplicates '("a" "A" "b" "a") :test #'equalp :from-end t)
(remove-duplicates '((1 . a) (2 . b) (1 . c)) :key #'car)
(remove-duplicates '(1 2 3 4 5) :test (lambda (a b) (= (mod a 2) (mod b 2))))
(remove-duplicates "abracadabra")
(remove-duplicates #(1 2 1 3) :start 1)
(length (make-list 100000))
