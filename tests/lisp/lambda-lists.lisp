;;; Lambda lists: &optional with defaults, &key, &aux, supplied-p.
;;;
;;; All of this is specified down to the detail — which scope a default form
;;; is evaluated in, what happens when the same keyword appears twice, whether
;;; an unsupplied parameter and an explicit nil can be told apart. Those are
;;; exactly the details worth checking against SBCL rather than guessing at.

;; &optional with no default is nil, as before.
((lambda (a &optional b) (list a b)) 1)
((lambda (a &optional b) (list a b)) 1 2)

;; A default form, evaluated only when nothing was supplied.
((lambda (a &optional (b 10)) (list a b)) 1)
((lambda (a &optional (b 10)) (list a b)) 1 2)
((lambda (a &optional (b 10)) (list a b)) 1 nil)

;; Each default is evaluated in a scope holding the parameters to its left.
((lambda (a &optional (b (* a 2))) (list a b)) 5)
((lambda (a &optional (b (* a 2)) (c (+ a b))) (list a b c)) 5)
((lambda (a &optional (b (* a 2)) (c (+ a b))) (list a b c)) 5 1)

;; A default is NOT evaluated when an argument was given.
(let ((n 0))
  ((lambda (&optional (x (setq n 1))) x) 'given)
  n)
(let ((n 0))
  ((lambda (&optional (x (setq n 1))) x))
  n)

;; supplied-p: the whole reason nil and "nothing" must differ.
((lambda (&optional (b 0 b-p)) (list b b-p)))
((lambda (&optional (b 0 b-p)) (list b b-p)) nil)
((lambda (&optional (b 0 b-p)) (list b b-p)) 7)

;; &key.
((lambda (&key b) b))
((lambda (&key b) b) :b 1)
((lambda (&key (b 9)) b))
((lambda (&key (b 9)) b) :b nil)
((lambda (&key (b 9 b-p)) (list b b-p)))
((lambda (&key (b 9 b-p)) (list b b-p)) :b nil)

;; Keywords are matched by name, so order does not matter.
((lambda (&key a b) (list a b)) :a 1 :b 2)
((lambda (&key a b) (list a b)) :b 2 :a 1)
((lambda (&key a b) (list a b)) :b 2)

;; CLHS: when a keyword appears more than once, the leftmost wins.
((lambda (&key a) a) :a 'first :a 'second)

;; A keyword name that differs from the variable name.
((lambda (&key ((:colour c) 'red)) c))
((lambda (&key ((:colour c) 'red)) c) :colour 'blue)

;; &allow-other-keys lets unknown keywords through.
((lambda (&key b &allow-other-keys) b) :b 1 :other 2)

;; Required, optional and key together.
((lambda (a &optional (b 2) &key (c 3)) (list a b c)) 1)
((lambda (a &optional (b 2) &key (c 3)) (list a b c)) 1 9)
((lambda (a &optional (b 2) &key (c 3)) (list a b c)) 1 9 :c 8)

;; &rest and &key see the same tail.
((lambda (a &rest r &key b) (list a r b)) 1 :b 2)
((lambda (a &rest r &key b) (list a r b)) 1)

;; &aux is a let*, written inside the lambda list.
((lambda (a &aux (d (* a a))) (list a d)) 4)
((lambda (a &aux b (c 1)) (list a b c)) 'x)
((lambda (a &aux (b a) (c (list b b))) c) 'y)

;; The same things through defun.
(defun greet (name &optional (greeting 'hello)) (list greeting name))
(greet 'world)
(greet 'world 'hi)

(defun describe-shape (kind &key (colour 'black) (sides 0) filled)
  (list kind colour sides filled))
(describe-shape 'square)
(describe-shape 'square :sides 4)
(describe-shape 'square :colour 'red :sides 4 :filled t)
(describe-shape 'circle :filled nil)

(defun running-total (x &aux (doubled (* x 2)) (tripled (* x 3)))
  (list x doubled tripled))
(running-total 3)

;; And through defmacro, where the arguments are code.
(defmacro twice-or (a &optional (b ''fallback)) (list 'list a b))
(twice-or 1)
(twice-or 1 2)

(defmacro tagged (x &key (tag ''none)) (list 'list x tag))
(tagged 1)
(tagged 1 :tag ''here)

;; A function whose defaults close over its own earlier arguments, called
;; several ways, because this is where an implementation gets the scope wrong.
(defun span (from &optional (to (+ from 10)) (step (- to from)))
  (list from to step))
(span 0)
(span 0 5)
(span 0 5 2)
(span 100)
