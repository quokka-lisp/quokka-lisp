;;;; Characters and strings: the essentials of CLHS chapters 13 and 16
;;;; (#54, step J3 of #39).
;;;;
;;;; A string is a vector of characters, so chapter 17's functions already
;;;; work on strings (J2). What is here is what is particular to text:
;;;; comparing characters and strings, with and without case; changing case;
;;;; trimming; and `char`/`schar`.
;;;;
;;;; Case-insensitive comparison folds both characters to lower case first, as
;;;; SBCL does. CLHS leaves open how a letter compares with a character that
;;;; sits between the two cases in the code table (`[`, `_`, `` ` ``), and the
;;;; tests do not ask.
;;;;
;;;; A string *designator* is a string, a symbol (its name) or a character (a
;;;; string of one). Every function below that takes a string takes one.

;;; ------------------------------------------------------------------------
;;; Characters

;;; `char=` and the rest take any number of characters, as CL's do — the
;;; prelude's `char=` took exactly two (decision 3). True when every adjacent
;;; pair is in order; `char/=` when no two are equal.
(defun %char-order (key test chars)
  (%pairwise (lambda (a b) (funcall test (funcall key a) (funcall key b)))
             (car chars) (cdr chars)))

(defun %char-code-folded (char) (char-code (char-downcase char)))

(defun char= (char &rest more) (%char-order #'char-code #'= (cons char more)))
(defun char< (char &rest more) (%char-order #'char-code #'< (cons char more)))
(defun char> (char &rest more) (%char-order #'char-code #'> (cons char more)))
(defun char<= (char &rest more) (%char-order #'char-code #'<= (cons char more)))
(defun char>= (char &rest more) (%char-order #'char-code #'>= (cons char more)))
(defun char-equal (char &rest more) (%char-order #'%char-code-folded #'= (cons char more)))
(defun char-lessp (char &rest more) (%char-order #'%char-code-folded #'< (cons char more)))
(defun char-greaterp (char &rest more) (%char-order #'%char-code-folded #'> (cons char more)))
(defun char-not-greaterp (char &rest more) (%char-order #'%char-code-folded #'<= (cons char more)))
(defun char-not-lessp (char &rest more) (%char-order #'%char-code-folded #'>= (cons char more)))

(defun char/= (char &rest more)
  (apply #'/= (char-code char) (mapcar #'char-code more)))
(defun char-not-equal (char &rest more)
  (apply #'/= (%char-code-folded char) (mapcar #'%char-code-folded more)))

;;; A character designator: a character, or a string or symbol of one.
(defun character (designator)
  (let ((text (if (characterp designator) nil (string designator))))
    (cond ((null text) designator)
          ((= (length text) 1) (char text 0))
          (t (error "not a character designator" designator)))))

;;; ------------------------------------------------------------------------
;;; Strings

;;; The string a designator stands for. Here rather than in the printer,
;;; which defined it first, because everything below uses it.
(defun string (designator)
  (cond ((stringp designator) designator)
        ((symbolp designator) (symbol-name designator))
        ((characterp designator) (make-string 1 :initial-element designator))
        (t (error "not a string designator" designator))))

;;; `make-string` accepts `:element-type` and ignores it: every string holds
;;; characters.
(defun make-string (size &key (initial-element #\Space) element-type)
  element-type
  (%make-string size initial-element))

(defun char (string index) (aref string index))
(defun schar (string index) (aref string index))
(defsetf char %set-aref)
(defsetf schar %set-aref)

;;; ------------------------------------------------------------------------
;;; Comparing strings
;;;
;;; Every comparison is the same walk: find the first position where the two
;;; ranges differ. That position, and which side was smaller there — or
;;; which ran out first — answers all twelve. The result of `string<` and
;;; its kin is that position, in the first string, or nil: a *generalised
;;; boolean* that also says where.

;;; The index in A where A's range and B's first differ, and :less, :equal
;;; or :greater for how A compares there.
(defun %string-compare (a b start1 end1 start2 end2 key)
  (let ((a (string a)) (b (string b)))
    (multiple-value-bind (s1 e1) (%bounds a start1 end1)
      (multiple-value-bind (s2 e2) (%bounds b start2 end2)
        (do ((i s1 (+ i 1))
             (j s2 (+ j 1)))
            ((or (>= i e1) (>= j e2))
             (values i (cond ((and (>= i e1) (>= j e2)) :equal)
                             ((>= i e1) :less)
                             (t :greater))))
          (let ((x (funcall key (char a i)))
                (y (funcall key (char b j))))
            (cond ((< x y) (return (values i :less)))
                  ((> x y) (return (values i :greater))))))))))

;;; Define a comparison that answers with the position when the order is one
;;; of ORDERS, and nil otherwise — or t, for `string=` and `string-equal`.
(defmacro %define-string-comparison (name key orders &optional boolean)
  `(defun ,name (string-1 string-2 &key (start1 0) end1 (start2 0) end2)
     (multiple-value-bind (index order)
         (%string-compare string-1 string-2 start1 end1 start2 end2 ,key)
       (if (member order ',orders)
           ,(if boolean t 'index)
           nil))))

(%define-string-comparison string= #'char-code (:equal) t)
(%define-string-comparison string/= #'char-code (:less :greater))
(%define-string-comparison string< #'char-code (:less))
(%define-string-comparison string> #'char-code (:greater))
(%define-string-comparison string<= #'char-code (:less :equal))
(%define-string-comparison string>= #'char-code (:greater :equal))
(%define-string-comparison string-equal #'%char-code-folded (:equal) t)
(%define-string-comparison string-not-equal #'%char-code-folded (:less :greater))
(%define-string-comparison string-lessp #'%char-code-folded (:less))
(%define-string-comparison string-greaterp #'%char-code-folded (:greater))
(%define-string-comparison string-not-greaterp #'%char-code-folded (:less :equal))
(%define-string-comparison string-not-lessp #'%char-code-folded (:greater :equal))

;;; ------------------------------------------------------------------------
;;; Changing case

;;; Apply FUNCTION to every character of STRING from START to END, in a
;;; fresh copy — or in STRING itself, for the `n` versions.
(defun %change-case (string start end function copy)
  (let ((out (if copy (copy-seq (string string)) string)))
    (multiple-value-bind (s e) (%bounds out start end)
      (do ((i s (+ i 1))) ((>= i e) out)
        (setf (char out i) (funcall function (char out i)))))))

(defun string-upcase (string &key (start 0) end)
  (%change-case string start end #'char-upcase t))
(defun string-downcase (string &key (start 0) end)
  (%change-case string start end #'char-downcase t))
(defun nstring-upcase (string &key (start 0) end)
  (%change-case string start end #'char-upcase nil))
(defun nstring-downcase (string &key (start 0) end)
  (%change-case string start end #'char-downcase nil))

;;; A *word* is a run of alphanumeric characters (CLHS): its first character
;;; is upcased and the rest downcased.
(defun %capitalize (string start end copy)
  (let ((out (if copy (copy-seq (string string)) string))
        (in-word nil))
    (multiple-value-bind (s e) (%bounds out start end)
      (do ((i s (+ i 1))) ((>= i e) out)
        (let ((c (char out i)))
          (if (alphanumericp c)
              (progn (setf (char out i) (if in-word (char-downcase c) (char-upcase c)))
                     (setq in-word t))
              (setq in-word nil)))))))

(defun string-capitalize (string &key (start 0) end) (%capitalize string start end t))
(defun nstring-capitalize (string &key (start 0) end) (%capitalize string start end nil))

;;; ------------------------------------------------------------------------
;;; Trimming

;;; STRING without the characters in CHARACTER-BAG at its LEFT and/or RIGHT.
(defun %trim (character-bag string left right)
  (let* ((string (string string))
         (start 0)
         (end (length string)))
    (if left
        (do () ((or (>= start end) (not (find (char string start) character-bag))))
          (setq start (+ start 1))))
    (if right
        (do () ((or (<= end start) (not (find (char string (- end 1)) character-bag))))
          (setq end (- end 1))))
    (subseq string start end)))

(defun string-trim (character-bag string) (%trim character-bag string t t))
(defun string-left-trim (character-bag string) (%trim character-bag string t nil))
(defun string-right-trim (character-bag string) (%trim character-bag string nil t))
