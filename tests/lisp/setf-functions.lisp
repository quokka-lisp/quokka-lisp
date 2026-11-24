;;; Setf functions (step AE, #78): a function named (setf name), which setf
;;; calls for a place (name ...) that has no other expansion, with the new
;;; value first. Also apply as a place, and the block around a local
;;; function's body. Tier 1: compiled and SBCL only. No function is printed.

(defun (setf second-of) (new list) (setf (car (cdr list)) new))
(let ((l (list 1 2 3))) (list (setf (second-of l) 'b) l))
(not (null (fboundp '(setf second-of))))
(functionp #'(setf second-of))
(functionp (fdefinition '(setf second-of)))
(let ((l (list 1 2 3))) (funcall #'(setf second-of) 'x l) l)

;; A setf function's body is in a block named after the accessor.
(defun (setf early) (new cell)
  (return-from early (setf (car cell) new))
  :never)
(let ((c (list 0))) (list (setf (early c) 5) c))

;; incf and push go through the same expansion.
(defun box-value (box) (car box))
(defun (setf box-value) (new box) (setf (car box) new))
(let ((b (list 10))) (incf (box-value b) 5) (push 'x (box-value b)) b)

;; Each subform once, left to right, then the value.
(let ((order nil) (b (list 1)))
  (setf (box-value (progn (push :place order) b)) (progn (push :value order) 2))
  (list (reverse order) b))

;; fmakunbound and fboundp of a setf name.
(defun (setf temporary-place) (new x) (list new x))
(not (null (fboundp '(setf temporary-place))))
(equal (fmakunbound '(setf temporary-place)) '(setf temporary-place))
(fboundp '(setf temporary-place))
(fboundp '(setf never-defined-anywhere))

;; (setf fdefinition) of a setf name.
(progn (setf (fdefinition '(setf pair-place)) #'cons) :defined)
(setf (pair-place 'a) 'b)

;; Local setf functions.
(flet (((setf head) (new list) (setf (car list) new)))
  (let ((l (list 1 2)))
    (setf (head l) 'a)
    l))
(labels (((setf tail-of) (new list) (setf (cdr list) new))
         (use (l) (setf (tail-of l) nil) l))
  (use (list 1 2 3)))
(flet (((setf head) (new list) (setf (car list) new)))
  (declare (ignore (function (setf head))))
  :good)

;; A local function's body is a block named after it.
(flet ((f () (return-from f 15) 35)) (f))
(labels ((g (n) (if (= n 0) (return-from g :done) (g (- n 1))))) (g 5))

;; A function name that is neither a symbol nor (setf symbol) is refused.
(handler-case (fboundp '(setf a b)) (type-error () :type-error))
(handler-case (fdefinition 42) (type-error () :type-error))

;; fdefinition of a special operator is a function, though not one to call.
(functionp (fdefinition 'if))

;; apply as a place.
(let ((v (vector 0 1 2 3))) (setf (apply #'aref v '(1)) 10) v)
(let ((a (make-array '(2 2) :initial-element 0))) (setf (apply #'aref a 1 1 nil) 'x) (aref a 1 1))
(let ((bv (copy-seq #*0000))) (setf (apply #'bit bv 2 nil) 1) bv)
(let ((b (list 1))) (setf (apply #'box-value b nil) 7) b)
