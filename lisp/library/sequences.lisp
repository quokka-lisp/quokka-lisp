;;;; Sequences: CLHS chapter 17 (#54, step J2 of #39).
;;;;
;;;; A *sequence* is a list or a vector, and a string is a vector of
;;;; characters. Every function here takes any of the three, and most take
;;;; the same keywords: `:key`, `:test`/`:test-not` (J1's `%test`), `:start`
;;;; and `:end` for a range, `:from-end` for the direction, `:count` for how
;;;; many.
;;;;
;;;; Decided in #54 (decision 4, Carsten): each function is written **once**,
;;;; over one iteration macro, `%do-sequence`, which expands into a `cdr`
;;;; loop for a list *and* an `aref` loop for a vector. Shorter than writing
;;;; each function twice, and unlike a run-time "next element" protocol it
;;;; costs no call per element — the specialising is done when the macro
;;;; expands, which is how SBCL's `seq-dispatch` works. To read `remove`,
;;;; read `%do-sequence` once first.
;;;;
;;;; Where CL *permits* a destructive function (`delete`, `nreverse`,
;;;; `nsubstitute`) to reuse its argument but does not require it, these
;;;; build a fresh result. A program may not rely on either.

;;; ------------------------------------------------------------------------
;;; The iteration macro

;;; `(%do-sequence (element sequence :start s :end e :from-end f :index i)
;;;    body...)`
;;;
;;; Run BODY with ELEMENT bound to each element of SEQUENCE from START up to
;;; END — backwards if FROM-END — and I, if given, to its index. `(return x)`
;;; in BODY stops and makes X the value; otherwise the value is nil.
;;;
;;; Four loops, because a list is walked by its tails and a vector by its
;;; indices, each in two directions. A list has no way back, so backwards
;;; over a list first gathers the range, `(index . element)`, in reverse.
;;;
;;; The options are a plist read with `getf`, because a macro lambda list
;;; cannot have `&key` inside a nested pattern yet (`lambda_list.rs`).
(defmacro %do-sequence ((element sequence &rest options) &body body)
  (let ((start (getf options :start 0)) (end (getf options :end))
        (from-end (getf options :from-end))
        (i (if (getf options :index) (getf options :index) (gensym "INDEX")))
        (seq (gensym "SEQ")) (s (gensym "START")) (e (gensym "END"))
        (tail (gensym "TAIL")) (pair (gensym "PAIR")))
    `(let ((,seq ,sequence))
       (multiple-value-bind (,s ,e) (%bounds ,seq ,start ,end)
         (if (listp ,seq)
             (if ,from-end
                 (dolist (,pair (%indexed-range-reversed ,seq ,s ,e))
                   (let ((,i (car ,pair)) (,element (cdr ,pair))) ,i ,@body))
                 (do ((,tail (nthcdr ,s ,seq) (cdr ,tail))
                      (,i ,s (+ ,i 1)))
                     ((>= ,i ,e))
                   (let ((,element (car ,tail))) ,@body)))
             (if ,from-end
                 (do ((,i (- ,e 1) (- ,i 1))) ((< ,i ,s))
                   (let ((,element (aref ,seq ,i))) ,@body))
                 (do ((,i ,s (+ ,i 1))) ((>= ,i ,e))
                   (let ((,element (aref ,seq ,i))) ,@body))))))))

;;; START and the real END of a range in SEQUENCE, or an error if it does
;;; not fit (CLHS 17.1.1).
(defun %bounds (sequence start end)
  (let* ((length (length sequence))
         (end (if end end length)))
    (if (or (not (integerp start)) (not (integerp end))
            (< start 0) (> end length) (> start end))
        (error "the range does not fit the sequence" (list start end length)))
    (values start end)))

(defun %indexed-range-reversed (list start end)
  (let ((out nil))
    (do ((tail (nthcdr start list) (cdr tail))
         (i start (+ i 1)))
        ((>= i end) out)
      (push (cons i (car tail)) out))))

;;; ------------------------------------------------------------------------
;;; Making sequences

(defun %kind-of (sequence)
  (cond ((listp sequence) :list)
        ((stringp sequence) :string)
        (t :vector)))

;;; The kind a type specifier asks for: `list`, `vector`, `string` and
;;; their usual spellings. There is no type system yet (phase 3), so this is
;;; a table, not `subtypep`.
(defun %kind-of-type (type)
  (let ((head (if (consp type) (car type) type)))
    (cond ((member head '(list cons null)) :list)
          ((member head '(string simple-string base-string simple-base-string)) :string)
          ((and (eq head 'vector) (consp type) (consp (cdr type))
                (member (car (cdr type)) '(character base-char standard-char)))
           :string)
          ((member head '(vector simple-vector array simple-array)) :vector)
          (t (error "not a sequence type Quokka knows" type)))))

;;; A sequence of KIND holding ITEMS, a list.
(defun %from-list (kind items)
  (cond ((eq kind :list) items)
        (t (let ((out (if (eq kind :string)
                          (make-string (length items))
                          (make-array (length items))))
                 (i 0))
             (dolist (item items out)
               (setf (aref out i) item)
               (setq i (+ i 1)))))))

;;; The elements from START to END, as a list.
(defun %range-list (sequence start end)
  (let ((out nil))
    (%do-sequence (element sequence :start start :end end)
      (push element out))
    (nreverse out)))

;;; SEQUENCE as something `aref` reads: itself if it is a vector.
(defun %as-vector (sequence)
  (if (listp sequence) (%from-list :vector sequence) sequence))

(defun make-sequence (type size &key initial-element)
  (%from-list (%kind-of-type type) (make-list size :initial-element initial-element)))

(defun concatenate (result-type &rest sequences)
  (let ((items nil))
    (dolist (sequence sequences)
      (%do-sequence (element sequence) (push element items)))
    (%from-list (%kind-of-type result-type) (nreverse items))))

;;; `coerce`, for sequences and floats only: the type system is phase 3.
;;; `t` asks for the object as it is.
(defun coerce (object type)
  (cond ((eq type t) object)
        ((member type '(float double-float single-float short-float long-float))
         (float object))
        ((and (eq (%kind-of-type type) :list) (listp object)) object)
        ((and (eq (%kind-of-type type) :string) (stringp object)) object)
        ((and (eq (%kind-of-type type) :vector) (vectorp object)) object)
        (t (%from-list (%kind-of-type type) (%range-list object 0 nil)))))

;;; ------------------------------------------------------------------------
;;; Reading

;;; `elt` with the bounds check the prelude's did not make (decision 3).
(defun elt (sequence index)
  (if (listp sequence)
      (let ((tail (if (and (integerp index) (>= index 0)) (nthcdr index sequence) nil)))
        (if (consp tail)
            (car tail)
            (error "index out of range for ELT" index)))
      (aref sequence index)))

(defun subseq (sequence start &optional end)
  (%from-list (%kind-of sequence) (%range-list sequence start end)))

(defsetf subseq (sequence start &optional end) (new)
  `(progn (replace ,sequence ,new :start1 ,start :end1 ,end) ,new))

;;; The prelude's `reverse` made a general vector out of a string (#54);
;;; this keeps the kind, as CL says (decision 3).
(defun reverse (sequence)
  (if (listp sequence)
      (revappend sequence nil)
      (%from-list (%kind-of sequence) (revappend (%range-list sequence 0 nil) nil))))

(defun nreverse (sequence) (reverse sequence))

;;; ------------------------------------------------------------------------
;;; Matching
;;;
;;; Three flavours of nearly every function — `find`, `find-if`,
;;; `find-if-not` — differ only in what counts as a match. Each makes a
;;; one-argument predicate and hands it to one worker.

(defun %matches (item key test test-not)
  (let ((same (%test test test-not)))
    (lambda (element) (funcall same item (%key key element)))))

(defun %satisfies (predicate key)
  (lambda (element) (funcall predicate (%key key element))))

(defun %fails (predicate key)
  (lambda (element) (not (funcall predicate (%key key element)))))

;;; find
(defun %find (matches sequence from-end start end)
  (%do-sequence (element sequence :start start :end end :from-end from-end)
    (if (funcall matches element) (return element))))

(defun find (item sequence &key from-end test test-not (start 0) end key)
  (%find (%matches item key test test-not) sequence from-end start end))
(defun find-if (predicate sequence &key from-end (start 0) end key)
  (%find (%satisfies predicate key) sequence from-end start end))
(defun find-if-not (predicate sequence &key from-end (start 0) end key)
  (%find (%fails predicate key) sequence from-end start end))

;;; position
(defun %position-of (matches sequence from-end start end)
  (%do-sequence (element sequence :start start :end end :from-end from-end :index i)
    (if (funcall matches element) (return i))))

(defun position (item sequence &key from-end test test-not (start 0) end key)
  (%position-of (%matches item key test test-not) sequence from-end start end))
(defun position-if (predicate sequence &key from-end (start 0) end key)
  (%position-of (%satisfies predicate key) sequence from-end start end))
(defun position-if-not (predicate sequence &key from-end (start 0) end key)
  (%position-of (%fails predicate key) sequence from-end start end))

;;; count
(defun %count (matches sequence start end)
  (let ((n 0))
    (%do-sequence (element sequence :start start :end end)
      (if (funcall matches element) (setq n (+ n 1))))
    n))

(defun count (item sequence &key from-end test test-not (start 0) end key)
  from-end
  (%count (%matches item key test test-not) sequence start end))
(defun count-if (predicate sequence &key from-end (start 0) end key)
  from-end
  (%count (%satisfies predicate key) sequence start end))
(defun count-if-not (predicate sequence &key from-end (start 0) end key)
  from-end
  (%count (%fails predicate key) sequence start end))

;;; ------------------------------------------------------------------------
;;; Removing and substituting
;;;
;;; Both mark the elements first — in a vector of flags, one per index,
;;; walking in the requested direction and stopping after COUNT — and then
;;; build the result in order. Marking first is what makes `:from-end` with
;;; `:count` right: `(remove 1 '(1 2 1 3 1) :count 2 :from-end t)` removes
;;; the *last* two.

(defun %marks (matches sequence from-end start end count)
  (let ((flags (make-array (length sequence)))
        (left count))
    (%do-sequence (element sequence :start start :end end :from-end from-end :index i)
      (if (and left (<= left 0)) (return nil))
      (when (funcall matches element)
        (setf (aref flags i) t)
        (if left (setq left (- left 1)))))
    flags))

(defun %without-marked (sequence flags)
  (let ((kept nil))
    (%do-sequence (element sequence :index i)
      (if (not (aref flags i)) (push element kept)))
    (%from-list (%kind-of sequence) (nreverse kept))))

(defun %replacing-marked (sequence flags new)
  (let ((out nil))
    (%do-sequence (element sequence :index i)
      (push (if (aref flags i) new element) out))
    (%from-list (%kind-of sequence) (nreverse out))))

(defun %remove (matches sequence from-end start end count)
  (%without-marked sequence (%marks matches sequence from-end start end count)))

(defun remove (item sequence &key from-end test test-not (start 0) end count key)
  (%remove (%matches item key test test-not) sequence from-end start end count))
(defun remove-if (predicate sequence &key from-end (start 0) end count key)
  (%remove (%satisfies predicate key) sequence from-end start end count))
(defun remove-if-not (predicate sequence &key from-end (start 0) end count key)
  (%remove (%fails predicate key) sequence from-end start end count))

(defun delete (item sequence &key from-end test test-not (start 0) end count key)
  (remove item sequence :from-end from-end :test test :test-not test-not
          :start start :end end :count count :key key))
(defun delete-if (predicate sequence &key from-end (start 0) end count key)
  (remove-if predicate sequence :from-end from-end :start start :end end :count count :key key))
(defun delete-if-not (predicate sequence &key from-end (start 0) end count key)
  (remove-if-not predicate sequence :from-end from-end :start start :end end :count count :key key))

(defun %substitute (new matches sequence from-end start end count)
  (%replacing-marked sequence (%marks matches sequence from-end start end count) new))

(defun substitute (new old sequence &key from-end test test-not (start 0) end count key)
  (%substitute new (%matches old key test test-not) sequence from-end start end count))
(defun substitute-if (new predicate sequence &key from-end (start 0) end count key)
  (%substitute new (%satisfies predicate key) sequence from-end start end count))
(defun substitute-if-not (new predicate sequence &key from-end (start 0) end count key)
  (%substitute new (%fails predicate key) sequence from-end start end count))

(defun nsubstitute (new old sequence &key from-end test test-not (start 0) end count key)
  (substitute new old sequence :from-end from-end :test test :test-not test-not
              :start start :end end :count count :key key))
(defun nsubstitute-if (new predicate sequence &key from-end (start 0) end count key)
  (substitute-if new predicate sequence :from-end from-end :start start :end end :count count :key key))
(defun nsubstitute-if-not (new predicate sequence &key from-end (start 0) end count key)
  (substitute-if-not new predicate sequence :from-end from-end :start start :end end :count count :key key))

;;; Of each set of duplicates in the range, the *last* is kept — or the
;;; first, with `:from-end` (CLHS).
(defun remove-duplicates (sequence &key from-end test test-not (start 0) end key)
  (multiple-value-bind (s e) (%bounds sequence start end)
    (let* ((same (%test test test-not))
           (range (%from-list :vector (%range-list sequence s e)))
           (n (length range))
           (flags (make-array (length sequence))))
      (dotimes (j n)
        (let ((this (%key key (aref range j))))
          (if (if from-end
                  (%some-index (lambda (k) (funcall same (%key key (aref range k)) this)) 0 j)
                  (%some-index (lambda (k) (funcall same this (%key key (aref range k)))) (+ j 1) n))
              (setf (aref flags (+ s j)) t))))
      (%without-marked sequence flags))))

(defun delete-duplicates (sequence &key from-end test test-not (start 0) end key)
  (remove-duplicates sequence :from-end from-end :test test :test-not test-not
                     :start start :end end :key key))

(defun %some-index (predicate from below)
  (do ((k from (+ k 1)))
      ((>= k below) nil)
    (if (funcall predicate k) (return t))))

;;; ------------------------------------------------------------------------
;;; Writing into a sequence

;;; Store ITEMS, a list, into SEQUENCE from START on.
(defun %store-range (sequence start items)
  (if (listp sequence)
      (do ((tail (nthcdr start sequence) (cdr tail))
           (rest items (cdr rest)))
          ((or (endp rest) (endp tail)) sequence)
        (rplaca tail (car rest)))
      (do ((i start (+ i 1))
           (rest items (cdr rest)))
          ((endp rest) sequence)
        (setf (aref sequence i) (car rest)))))

(defun fill (sequence item &key (start 0) end)
  (multiple-value-bind (s e) (%bounds sequence start end)
    (%store-range sequence s (make-list (- e s) :initial-element item))))

;;; The source range is copied out before anything is written, so a
;;; sequence replaced into itself comes out as if from a copy (CLHS).
(defun replace (sequence-1 sequence-2 &key (start1 0) end1 (start2 0) end2)
  (multiple-value-bind (s1 e1) (%bounds sequence-1 start1 end1)
    (let ((source (%range-list sequence-2 start2 end2)))
      (%store-range sequence-1 s1
                    (%range-list source 0 (min (- e1 s1) (length source)))))))

(defun map-into (result function &rest sequences)
  (let* ((vectors (mapcar #'%as-vector sequences))
         (n (apply #'min (length result) (mapcar #'length vectors)))
         (items nil))
    (dotimes (i n)
      (push (apply function (mapcar (lambda (v) (aref v i)) vectors)) items))
    (%store-range result 0 (nreverse items))))

;;; ------------------------------------------------------------------------
;;; Mapping, reducing, and quantifiers
;;;
;;; Several sequences at once, stopping at the shortest: each is made
;;; something `aref` can index, and they are walked by a shared index.

(defun %map-rows (function sequences)
  (let* ((vectors (mapcar #'%as-vector sequences))
         (n (apply #'min (mapcar #'length vectors)))
         (out nil))
    (dotimes (i n (nreverse out))
      (push (apply function (mapcar (lambda (v) (aref v i)) vectors)) out))))

(defun map (result-type function sequence &rest more)
  (let ((items (%map-rows function (cons sequence more))))
    (if (null result-type) nil (%from-list (%kind-of-type result-type) items))))

;;; Stop at the first row whose answer is DECISIVE (true for `some`), and
;;; return what that row said.
(defun %first-row (function sequences decisive)
  (let* ((vectors (mapcar #'%as-vector sequences))
         (n (apply #'min (mapcar #'length vectors))))
    (dotimes (i n nil)
      (let ((answer (apply function (mapcar (lambda (v) (aref v i)) vectors))))
        (if (if decisive answer (not answer)) (return (list answer)))))))

(defun some (predicate sequence &rest more)
  (car (%first-row predicate (cons sequence more) t)))
(defun every (predicate sequence &rest more)
  (not (%first-row predicate (cons sequence more) nil)))
(defun notany (predicate sequence &rest more)
  (not (%first-row predicate (cons sequence more) t)))
(defun notevery (predicate sequence &rest more)
  (if (%first-row predicate (cons sequence more) nil) t nil))

;;; `(reduce f '(1 2 3))` is `(f (f 1 2) 3)`; with `:from-end`,
;;; `(f 1 (f 2 3))`. An empty range calls F with no arguments; one element,
;;; and no initial value, is returned without calling it.
(defun reduce (function sequence &key key from-end (start 0) end
                                      (initial-value nil initial-p))
  (let ((items nil))
    ;; Gathered backwards, so ITEMS ends up in the order the fold wants.
    (%do-sequence (element sequence :start start :end end :from-end (not from-end))
      (push (%key key element) items))
    (if initial-p (push initial-value items))
    (cond ((null items) (funcall function))
          ((null (cdr items)) (car items))
          (t (let ((acc (car items)))
               (dolist (item (cdr items) acc)
                 (setq acc (if from-end
                               (funcall function item acc)
                               (funcall function acc item)))))))))

;;; ------------------------------------------------------------------------
;;; Searching

;;; Where the elements of SEQUENCE-1 and SEQUENCE-2 first differ, as an
;;; index into SEQUENCE-1, or nil if the two ranges are the same. With
;;; `:from-end`, one past where they last differ, counting from the ends.
(defun mismatch (sequence-1 sequence-2 &key from-end test test-not key
                                            (start1 0) end1 (start2 0) end2)
  (multiple-value-bind (s1 e1) (%bounds sequence-1 start1 end1)
    (multiple-value-bind (s2 e2) (%bounds sequence-2 start2 end2)
      (let* ((same (%test test test-not))
             (a (%as-vector sequence-1)) (b (%as-vector sequence-2))
             (n1 (- e1 s1)) (n2 (- e2 s2)) (m (min n1 n2)))
        (flet ((differ (i j) (not (funcall same (%key key (aref a i)) (%key key (aref b j))))))
          (if from-end
              (do ((k 1 (+ k 1)))
                  ((> k m) (if (= n1 n2) nil (- e1 m)))
                (if (differ (- e1 k) (- e2 k)) (return (+ (- e1 k) 1))))
              (do ((k 0 (+ k 1)))
                  ((>= k m) (if (= n1 n2) nil (+ s1 m)))
                (if (differ (+ s1 k) (+ s2 k)) (return (+ s1 k))))))))))

;;; Where SEQUENCE-1's range first occurs in SEQUENCE-2, as an index into
;;; SEQUENCE-2 — the last occurrence with `:from-end`. Every position is
;;; tried in turn: the plain algorithm, quadratic in the worst case, and
;;; plenty until a profile says otherwise.
(defun search (sequence-1 sequence-2 &key from-end test test-not key
                                          (start1 0) end1 (start2 0) end2)
  (multiple-value-bind (s1 e1) (%bounds sequence-1 start1 end1)
    (multiple-value-bind (s2 e2) (%bounds sequence-2 start2 end2)
      (let* ((same (%test test test-not))
             (a (%as-vector sequence-1)) (b (%as-vector sequence-2))
             (n (- e1 s1)) (found nil))
        (flet ((here-p (p)
                 (do ((k 0 (+ k 1)))
                     ((>= k n) t)
                   (if (not (funcall same (%key key (aref a (+ s1 k))) (%key key (aref b (+ p k)))))
                       (return nil)))))
          (do ((p s2 (+ p 1)))
              ((> (+ p n) e2) found)
            (when (here-p p)
              (setq found p)
              (if (not from-end) (return p)))))))))

;;; ------------------------------------------------------------------------
;;; Sorting
;;;
;;; Merge sort: split in half, sort each, merge (von Neumann, 1945). It is
;;; *stable* — equal elements keep their order — and O(n log n) in every
;;; case, which is why it is the textbook sort for lists. `sort` uses it too:
;;; CL lets `sort` be unstable, never requires it to be.

;;; Merge two sorted lists. On a tie the element from A comes first, which is
;;; what keeps the sort stable.
(defun %merge-lists (a b predicate key)
  (let ((out nil))
    (do () ((or (endp a) (endp b)) (revappend out (if (endp a) b a)))
      (if (funcall predicate (%key key (car b)) (%key key (car a)))
          (progn (push (car b) out) (setq b (cdr b)))
          (progn (push (car a) out) (setq a (cdr a)))))))

(defun %merge-sort (list predicate key)
  (let ((n (length list)))
    (if (< n 2)
        list
        (let ((half (floor n 2)))
          (%merge-lists (%merge-sort (subseq list 0 half) predicate key)
                        (%merge-sort (nthcdr half list) predicate key)
                        predicate key)))))

;;; A vector is sorted in place, as CL lets it be, and returned.
(defun stable-sort (sequence predicate &key key)
  (let ((sorted (%merge-sort (%range-list sequence 0 nil) predicate key)))
    (if (listp sequence)
        sorted
        (%store-range sequence 0 sorted))))

(defun sort (sequence predicate &key key)
  (stable-sort sequence predicate :key key))

(defun merge (result-type sequence-1 sequence-2 predicate &key key)
  (%from-list (%kind-of-type result-type)
              (%merge-lists (%range-list sequence-1 0 nil)
                            (%range-list sequence-2 0 nil)
                            predicate key)))
