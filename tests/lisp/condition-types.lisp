;;; Step AB (#75): the condition types CLHS names for the library's own
;;; checks, and the checks that were missing. Each case catches exactly the
;;; type CLHS names; anything else would escape and fail. Tier 1: compiled
;;; and SBCL only.

;; elt: an index outside the sequence, a list's end or a vector's fill pointer.
(handler-case (elt '(a b) 5) (type-error () :type-error))
(handler-case (elt '(a b) -1) (type-error () :type-error))
(handler-case (elt (vector 1 2) 2) (type-error () :type-error))
;; Past the fill pointer Quokka signals too, as ansi-test asks at safety 3;
;; SBCL checks it only at high safety, so that case is not compared here.
(elt (make-array 5 :initial-element 7 :fill-pointer 3) 2)
(handler-case (setf (elt (make-array 5 :initial-element 0 :fill-pointer 3) 4) 'x)
  (type-error () :type-error))

;; A count that is not a non-negative integer.
(handler-case (nthcdr -10 (list 'a 'b)) (type-error () :type-error))
(handler-case (nthcdr 'a (list 'a 'b)) (type-error () :type-error))
(handler-case (last (list 'a 'b 'c) -1) (type-error () :type-error))
(handler-case (last (list 'a 'b 'c) 10.0) (type-error () :type-error))
(handler-case (butlast (list 'a 'b) -2) (type-error () :type-error))
(handler-case (nbutlast (list 'a) 20.0) (type-error () :type-error))
(list (last '(a b c) 2) (butlast '(a b c) 2) (nthcdr 1 '(a b c)))

;; endp of something that is not a list.
(handler-case (endp 1) (type-error () :type-error))
(list (endp nil) (endp '(a)))

;; A type that is not a sequence type.
(handler-case (make-sequence 'symbol 3) (type-error () :type-error))
(handler-case (map 'symbol #'identity '(a b c)) (type-error () :type-error))

;; Packages: a name with no package, and something that names none.
(handler-case (package-name "NO-SUCH-PACKAGE-HERE") (package-error () :package-error))
(handler-case (package-nicknames "NO-SUCH-PACKAGE-HERE") (package-error () :package-error))
(handler-case (eval '(in-package "NO-SUCH-PACKAGE-HERE")) (package-error () :package-error))
(handler-case (package-nicknames 10) (type-error () :type-error))

;; fdefinition of something that is not a function name.
(handler-case (fdefinition 10) (type-error () :type-error))
(handler-case (fdefinition '(setf no-such-function-here)) (undefined-function () :undefined))

;; gensym with a counter that is not a non-negative integer.
(handler-case (let ((*gensym-counter* -1)) (gensym)) (type-error () :type-error))

;; nil in a destructuring pattern is the empty pattern, matching only nil;
;; a value that does not fit its pattern is a program-error.
(handler-case (destructuring-bind (foo nil bar) (list 1 2 3) (list foo bar))
  (program-error () :program-error))
(destructuring-bind (foo nil bar) (list 1 nil 3) (list foo bar))
(destructuring-bind (a (b nil) c) (list 1 (list 2 nil) 3) (list a b c))
(handler-case (destructuring-bind (a b c) nil (list a b c)) (program-error () :program-error))
(handler-case (destructuring-bind (a) (list 1 2) a) (program-error () :program-error))
(destructuring-bind (a &optional (b 5)) (list 1) (declare (ignorable a)) (list a b))
