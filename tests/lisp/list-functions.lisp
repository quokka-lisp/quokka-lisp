;;;; Equality, control and iteration macros, and the list functions of CLHS
;;;; chapter 14 (#54, step J1).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.
;;;; Where CLHS leaves an order unspecified (union, intersection, pairlis),
;;;; the tests compare as sets or look things up rather than print the list.

;; Equality.
(list (eql 'a 'a) (eql 3 3) (eql #\a #\a) (eql "ab" "ab") (eql '(1) '(1)))
(list (equal "ab" "ab") (equal '(1 (2 "x")) '(1 (2 "x"))) (equal "ab" "AB") (equal '(1 . 2) '(1 . 2)))
(list (equalp "ab" "AB") (equalp #\a #\A) (equalp #(1 "x") #(1 "X")) (equalp '(1 #\a) '(1 #\A)) (equalp 1 2))

;; Functions about functions.
(list (identity 4) (funcall (constantly 'k) 1 2 3) (funcall (complement #'consp) 5))

;; prog1, prog2, case, ecase.
(let ((x 1)) (list (prog1 x (setq x 2)) x))
(prog2 'a 'b 'c)
(mapcar (lambda (x) (case x (1 'one) ((2 3) 'two-or-three) (otherwise 'many))) '(1 2 3 4))
(case 'b ((a) 1) ((b c) 2) (t 3))
(case 5 (1 'one))
(case nil ((nil) 'empty-list-key) (t 'other))
(ecase 2 (1 'one) (2 'two))
(handler-case (ecase 9 (1 'one)) (error () 'no-match))

;; destructuring-bind.
(destructuring-bind (a (b c) &optional (d 4) &rest e) '(1 (2 3)) (list a b c d e))
(destructuring-bind (a &key (k 'default) j) '(1 :j 2) (list a k j))
(destructuring-bind (a . b) '(1 2 3) (list a b))
(handler-case (destructuring-bind (a b) '(1) (list a b)) (error () 'too-short))

;; Iteration: dolist, dotimes, do, do*, and return out of each.
(let ((sum 0)) (dolist (x '(1 2 3) sum) (setq sum (+ sum x))))
(dolist (x '(1 2 3)))
(dolist (x '(1 2 3 4)) (if (= x 3) (return (* x 10))))
(let ((out nil)) (dotimes (i 4 out) (push i out)))
(dotimes (i 3 i))
(dotimes (i 10) (if (= i 4) (return 'early)))
(do ((i 0 (+ i 1)) (acc nil (cons i acc))) ((= i 4) acc))
(do* ((i 0 (+ i 1)) (j 0 i)) ((= i 3) (list i j)))
(do ((a 1 b) (b 2 a) (n 0 (+ n 1))) ((= n 3) (list a b)))
(let ((n 0)) (do () ((> n 2) n) (setq n (+ n 1))))
(let ((out nil)) (dolist (x '(1 2 3)) (if (= x 2) (go skip)) (push x out) skip) out)
(let ((a 1) (b 2)) (psetq a b b a) (list a b))

;; A local function shadows a global macro of the same name.
(defmacro shout (x) (list 'quote (list 'macro x)))
(list (shout 1) (flet ((shout (x) (list 'local x))) (shout 1)))

;; Conses.
(list (atom 1) (atom nil) (atom '(1)))
(list (caaar '(((1)))) (cdadr '(0 (1 2))) (cdddr '(1 2 3 4)) (cadddr '(1 2 3 4)) (cddddr '(1 2 3 4 5)))
(list (fourth '(1 2 3 4 5)) (fifth '(1 2 3 4 5)) (tenth '(1 2 3 4 5 6 7 8 9 10)) (sixth '(1 2)))
(let ((x (list 1 2 3 4 5 6))) (setf (fourth x) 'd (sixth x) 'f (cdddr x) '(z)) x)
(list (list* 1 2 '(3 4)) (list* 1) (list* 1 2))
(make-list 3 :initial-element 'x)
(list (nthcdr 2 '(1 2 3)) (nthcdr 5 '(1 2 3)) (nthcdr 0 '(1)))
(list (list-length '(1 2 3)) (list-length nil) (let ((c (list 1 2))) (setf (cdr (cdr c)) c) (list-length c)))
(list (last '(1 2 3)) (last '(1 2 3) 2) (last '(1 2 3) 0) (last nil))
(list (butlast '(1 2 3)) (butlast '(1 2 3) 2) (butlast '(1 2) 5) (nbutlast (list 1 2 3)))
(let* ((a (list 1 (list 2) 3)) (b (copy-list a)) (c (copy-tree a)))
  (list (equal a b) (eq a b) (eq (second a) (second b)) (eq (second a) (second c))))
(copy-list '(1 2 . 3))
(let ((alist (list (cons 'a 1)))) (eq (car alist) (car (copy-alist alist))))
(list (tree-equal '(1 (2)) '(1 (2))) (tree-equal '(1 (2)) '(1 (3))) (tree-equal '("a") '("A") :test #'equalp))
(nconc (list 1 2) nil (list 3) (list 4 5))
(let ((x (list 1 2))) (nconc x (list 3)) x)
(list (nconc) (nconc nil nil) (nconc (list 1) 2))
(list (revappend '(1 2) '(3 4)) (nreconc (list 1 2) '(3)))
(let* ((x (list 1 2 3)) (tail (cddr x))) (list (tailp tail x) (tailp '(3) x) (ldiff x tail) (ldiff x '(3))))

;; Mapping.
(mapcar #'+ '(1 2 3) '(10 20 30 40))
(mapcar (lambda (x) (* x x)) '(1 2 3))
(let ((out nil)) (list (mapc (lambda (x) (push x out)) '(1 2)) out))
(mapcan (lambda (x) (if (> x 1) (list x x))) '(1 2 3))
(maplist #'identity '(1 2 3))
(let ((out nil)) (mapl (lambda (tail) (push (length tail) out)) '(a b c)) out)
(mapcon (lambda (tail) (list (length tail))) '(a b c))

;; Membership and association.
(list (member 2 '(1 2 3)) (member 5 '(1 2 3)) (member "b" '("a" "b") :test #'equal)
      (member 2 '((1) (2) (3)) :key #'car) (member 1 '(1 2 3) :test-not #'=))
(list (member-if #'consp '(1 (2) 3)) (member-if-not #'integerp '(1 a 2)))
(list (adjoin 1 '(1 2)) (adjoin 3 '(1 2)) (adjoin '(1 x) '((1 y)) :key #'car))
(let ((items (list 1 2))) (pushnew 2 items) (pushnew 3 items) items)
(acons 'a 1 '((b . 2)))
(let ((alist (pairlis '(a b c) '(1 2 3)))) (list (cdr (assoc 'b alist)) (length alist)))
(list (assoc 'b '((a . 1) nil (b . 2))) (assoc "b" '(("a" . 1) ("b" . 2)) :test #'equal)
      (assoc 2 '((1 . a) (3 . b)) :test #'<) (assoc 'z '((a . 1))))
(list (assoc-if #'evenp '((1 . a) (2 . b))) (assoc-if-not #'oddp '((1 . a) (2 . b))))
(list (rassoc 2 '((a . 1) (b . 2))) (rassoc-if #'evenp '((a . 1) (b . 2))) (rassoc-if-not #'oddp '((a . 1) (b . 2))))

;; Trees.
(subst 'x 'b '(a b (c b)))
(subst 'x "b" '("a" ("b")) :test #'equal)
(subst-if 0 #'integerp '(a 1 (b 2)))
(subst-if-not 'atom #'consp '(a (b)))
(sublis '((a . 1) (b . 2)) '(a (b c) . a))
(nsubst 'x 'b (list 'a 'b))

;; Lists as sets — compared as sets, since the order is unspecified.
(defun same-set-p (a b) (and (subsetp a b) (subsetp b a)))
(same-set-p (union '(1 2 3) '(2 3 4)) '(1 2 3 4))
(same-set-p (intersection '(1 2 3) '(2 3 4)) '(2 3))
(same-set-p (set-difference '(1 2 3) '(2 3 4)) '(1))
(same-set-p (set-exclusive-or '(1 2 3) '(2 3 4)) '(1 4))
(list (subsetp '(1 2) '(3 2 1)) (subsetp '(1 5) '(1 2)) (subsetp nil nil))
(length (union '((1 a) (2 b)) '((1 c)) :key #'car))
(same-set-p (nunion (list 1 2) (list 2 3)) '(1 2 3))
(same-set-p (nintersection (list 1 2) (list 2 3)) '(2))
(same-set-p (nset-difference (list 1 2) (list 2 3)) '(1))

;; The number comparisons J1 needed, added to numbers.lisp.
(list (<= 1 2 2) (<= 3 2) (>= 3 2 2) (>= 3 2 5) (/= 1 2 3) (/= 1 2 1) (/= 7))
(list (1+ 4) (1- 4) (evenp 0) (evenp -2) (oddp -3) (oddp 4) (max 1 5 3) (min 4 -2 7) (max 2))
