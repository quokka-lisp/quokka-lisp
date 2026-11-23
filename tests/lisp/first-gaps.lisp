;;;; The first gaps ansi-test found (#62, step O).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.

;; 1. A bad keyword argument is a program-error a handler can catch, and
;; :allow-other-keys at the call lets unknown keywords through.
(defun keys (&key a b) (list a b))
(handler-case (keys :c 1) (program-error () :caught))
(handler-case (keys :a) (program-error () :caught))
(keys :a 1 :allow-other-keys t :c 3)
(keys :allow-other-keys nil :b 2)
(handler-case (keys :allow-other-keys nil :c 3) (program-error () :caught))
(keys :a 1 :a 2)

;; 2. defstruct names its functions in the current package.
(progn (defpackage :first-gaps (:use :common-lisp)) t)
(progn (in-package :first-gaps) t)
(defstruct spot x y)
(let ((s (make-spot :x 1 :y 2)))
  (list (spot-x s) (spot-p s) (spot-y (copy-spot s))
        (eq (symbol-package 'make-spot) (find-package :first-gaps))))
(progn (in-package :cl-user) t)

;; 3. A symbol called as a function means its global function.
(list (funcall 'car '(1 2)) (apply 'list 1 '(2 3)) (mapcar 'abs '(-1 2 -3))
      (member 2 '(1 2 3) :test 'eql) (reduce '+ '(1 2 3)))
(handler-case (funcall 'no-such-function-anywhere) (undefined-function () :undefined))
(handler-case (funcall 'when t) (undefined-function () :undefined))

;; 4. #+, #- and #. read by features and at read time.
(list 1 #+common-lisp 2 #-common-lisp 3 #+(or) 4 #+(and) 5 #-(and common-lisp (not common-lisp)) 6)
'(a #+nonexistent-feature (nonexistent-package:symbol 1/2 #C(1 2)) b)
(list #.(+ 1 2) '#.(list 'x 'y))
(let ((*read-eval* nil))
  (handler-case (read-from-string "#.(+ 1 2)") (error () :refused)))
(read-from-string "#+(not common-lisp) skipped kept")

;; 5. Declarations: ignored, except special.
(let ((x 1)) (declare (ignore x)) 5)
(let ((n 3)) (declare (type fixnum n) (optimize (speed 1))) (* n n))
(defun doubled (x) "Twice X." (declare (type number x)) (* 2 x))
(doubled 21)
(defun free-y () (declare (special y)) y)
(let ((y 10)) (declare (special y)) (free-y))
(let ((y 1)) (declare (special y)) (let ((y 2)) (list y (free-y))))
(defun bind-z (z) (declare (special z)) (free-z))
(defun free-z () (declare (special z)) z)
(bind-z 42)
(let* ((a 1) (b (+ a 1))) (declare (special b)) (symbol-value 'b))
(defun optional-q (&optional (q 3)) (declare (special q)) (free-q))
(defun free-q () (declare (special q)) q)
(optional-q)
(let ((w 1))
  (let ((w 2))
    (locally (declare (special w)) (handler-case w (unbound-variable () :unbound)))))
(progn (declaim (special *first-gaps-dd*)) t)
(defun free-dd () *first-gaps-dd*)
(let ((*first-gaps-dd* 7)) (free-dd))
(progn (proclaim '(special *first-gaps-ee*)) t)
(let ((*first-gaps-ee* 8)) (symbol-value '*first-gaps-ee*))
(locally (declare (optimize (speed 3))) 9)
