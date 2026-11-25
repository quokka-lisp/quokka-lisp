;;;; Basic format (#57, step K of #39).
;;;;
;;;; `(format destination control-string argument...)`. The control string is
;;;; text with directives in it, each introduced by `~`, and each directive
;;;; uses up arguments or shapes the output: `(format nil "~A has ~D item~:P"
;;;; "cart" 3)` is "cart has 3 items".
;;;;
;;;; Decided in #57 (Carsten):
;;;;
;;;; 1. "Basic" is `~A ~S ~D ~B ~O ~X ~nR ~C ~% ~& ~| ~~ ~P ~* ~[ ~{ ~( ~? ~^`
;;;;    and tilde-newline, with parameters as numbers, 'c, V and #. English
;;;;    and Roman numerals, tabulation, justification and the pretty
;;;;    printer's directives wait for tier 2. The float directives `~F ~E ~G
;;;;    ~$` came with floats (step M, #17), at the end of this file.
;;;; 2. Control strings are interpreted, every time. Compiling literal ones,
;;;;    as SBCL does, is #58, for a later phase.
;;;; 3. Parse, then run: the control string becomes a tree of literal strings
;;;;    and directive records, and a second function walks the tree with the
;;;;    arguments — the reader and evaluator split again (chapters 8, 9).

;;; ------------------------------------------------------------------------
;;; Parsing

;;; One directive: its character (upcased), its parameters (a list, nil for
;;; one left empty), its modifiers, and for `~[ ~{ ~(` the clauses inside it,
;;; each itself a list of items. DEFAULT is true when `~[`'s last clause
;;; follows `~:;`; ONCE when `~{` was closed by `~:}`.
(defstruct %format-directive char params colon at clauses default once)

;;; The items of CONTROL, a list of strings and directives.
(defun %parse-control (control)
  (multiple-value-bind (items next closer) (%parse-items control 0)
    next
    (if closer
        (error "format: a closing directive with nothing to close" control))
    items))

;;; Items from START until the end of CONTROL or a closing directive — one of
;;; `~; ~] ~} ~)` — which is returned, unconsumed by any item, as the third
;;; value, after the index just past it.
(defun %parse-items (control start)
  (let ((items nil) (text nil) (i start) (end (length control)))
    (flet ((flush ()
             (when text
               (push (coerce (reverse text) 'string) items)
               (setq text nil))))
      (do () ((>= i end) (flush) (values (reverse items) i nil))
        (let ((c (char control i)))
          (if (not (char= c #\~))
              (progn (push c text) (setq i (+ i 1)))
              (multiple-value-bind (directive next) (%parse-directive control (+ i 1))
                (setq i next)
                (let ((d (%format-directive-char directive)))
                  (cond
                    ;; A closing directive ends this list of items.
                    ((member d '(#\; #\] #\} #\)))
                     (flush)
                     (return (values (reverse items) i directive)))
                    ;; Tilde-newline: the newline, and the whitespace after
                    ;; it, disappear (CLHS 22.3.9.3); `:` keeps the
                    ;; whitespace, `@` keeps the newline.
                    ((char= d #\Newline)
                     (if (%format-directive-at directive) (push #\Newline text))
                     (if (not (%format-directive-colon directive))
                         (do () ((or (>= i end) (not (member (char control i) '(#\Space #\Tab)))))
                           (setq i (+ i 1)))))
                    ((member d '(#\[ #\{ #\())
                     (flush)
                     (setq i (%parse-nested control i directive))
                     (push directive items))
                    (t (flush) (push directive items)))))))))))

;;; Read the clauses of a `~[`, `~{` or `~(` that ends at I, into DIRECTIVE,
;;; and return the index after its closing directive.
(defun %parse-nested (control i directive)
  (let ((close (cdr (assoc (%format-directive-char directive)
                           '((#\[ . #\]) (#\{ . #\}) (#\( . #\))))))
        (clauses nil))
    (do () (nil)
      (multiple-value-bind (items next closer) (%parse-items control i)
        (push items clauses)
        (setq i next)
        (cond ((null closer) (error "format: no closing directive" control close))
              ((char= (%format-directive-char closer) close)
               (if (%format-directive-colon closer) (setf (%format-directive-once directive) t))
               (setf (%format-directive-clauses directive) (reverse clauses))
               (return i))
              ((and (char= (%format-directive-char closer) #\;) (char= close #\]))
               ;; `~:;` before the last clause makes it the default.
               (if (%format-directive-colon closer) (setf (%format-directive-default directive) t)))
              (t (error "format: a misplaced closing directive" control)))))))

;;; A directive starting at I, just after its `~`: parameters, modifiers,
;;; character. Returns the directive and the index after it.
(defun %parse-directive (control i)
  (let ((params nil) (colon nil) (at nil) (end (length control)))
    ;; Parameters: comma-separated, each a number, 'c, V or #, or empty.
    (do ((more t)) ((not more))
      (multiple-value-bind (param next) (%parse-parameter control i)
        (setq i next)
        (push param params)
        (if (and (< i end) (char= (char control i) #\,))
            (setq i (+ i 1))
            (setq more nil))))
    (setq params (reverse params))
    (if (equal params '(nil)) (setq params nil))
    (do () ((not (and (< i end) (member (char control i) '(#\: #\@)))))
      (if (char= (char control i) #\:) (setq colon t) (setq at t))
      (setq i (+ i 1)))
    (if (>= i end) (error "format: the control string ends inside a directive" control))
    (values (make-%format-directive :char (char-upcase (char control i))
                                    :params params :colon colon :at at)
            (+ i 1))))

;;; One parameter at I: a number, `'c`, `V`, `#`, or nothing. Returns it
;;; (an integer, a character, :v, :count or nil) and the index after it.
(defun %parse-parameter (control i)
  (let ((end (length control)))
    (cond ((>= i end) (values nil i))
          ((char= (char control i) #\') (values (char control (+ i 1)) (+ i 2)))
          ((char-equal (char control i) #\V) (values :v (+ i 1)))
          ((char= (char control i) #\#) (values :count (+ i 1)))
          ((or (digit-char-p (char control i)) (member (char control i) '(#\+ #\-)))
           (let ((j (+ i 1)))
             (do () ((or (>= j end) (not (digit-char-p (char control j)))))
               (setq j (+ j 1)))
             (values (read-from-string (subseq control i j)) j)))
          (t (values nil i)))))

;;; ------------------------------------------------------------------------
;;; Running

;;; The arguments as `format` consumes them: all of them, for `~@*` and `#`,
;;; and those not yet used.
(defstruct %format-args all rest)

(defun %next-arg (args)
  (if (null (%format-args-rest args))
      (error "format: not enough arguments"))
  (pop (%format-args-rest args)))

(defun %args-left (args) (length (%format-args-rest args)))

;;; The parameters of DIRECTIVE with `V` and `#` resolved, padded with
;;; DEFAULTS where absent.
(defun %params (directive args defaults)
  (let ((given (%format-directive-params directive)) (out nil))
    (dolist (default defaults (reverse out))
      (let ((p (car given)))
        (setq given (cdr given))
        (push (cond ((eq p :v) (let ((v (%next-arg args))) (if v v default)))
                    ((eq p :count) (%args-left args))
                    ((null p) default)
                    (t p))
              out)))))

;;; Where a ~^ goes: the catch tag of the innermost construct it ends — the
;;; whole format, a ~{ iteration, or one step of a ~:{. Bound by each.
(defvar %format-escape-tag nil)

;;; Inside a ~:{, the list of sublists, for ~:^, which ends the iteration
;;; when the current step is the last.
(defvar %format-steps nil)

;;; Run ITEMS against ARGS, writing to STREAM.
(defun %run-items (items stream args)
  (dolist (item items)
    (if (stringp item)
        (write-string item stream)
        (%run-directive item stream args))))

(defun %run-directive (directive stream args)
  (let ((c (%format-directive-char directive))
        (colon (%format-directive-colon directive))
        (at (%format-directive-at directive)))
    (cond
      ((char= c #\A) (%format-padded directive stream args nil))
      ((char= c #\S) (%format-padded directive stream args t))
      ((char= c #\D) (%format-integer directive stream args 10 nil))
      ((char= c #\B) (%format-integer directive stream args 2 nil))
      ((char= c #\O) (%format-integer directive stream args 8 nil))
      ((char= c #\X) (%format-integer directive stream args 16 nil))
      ((char= c #\R)
       (if (%format-directive-params directive)
           (%format-integer directive stream args nil t)
           (error "format: ~R without a radix (English and Roman numerals) is not supported")))
      ((char= c #\C) (%format-character (%next-arg args) stream colon at))
      ((char= c #\F) (%format-fixed directive stream args))
      ((char= c #\E) (%format-exponential directive stream args))
      ((char= c #\G) (%format-general directive stream args))
      ((char= c #\$) (%format-money directive stream args))
      ((char= c #\%) (dotimes (i (car (%params directive args '(1)))) (terpri stream)))
      ((char= c #\&)
       (let ((n (car (%params directive args '(1)))))
         (when (> n 0)
           (fresh-line stream)
           (dotimes (i (- n 1)) (terpri stream)))))
      ((char= c #\|) (dotimes (i (car (%params directive args '(1)))) (write-char (code-char 12) stream)))
      ((char= c #\~) (dotimes (i (car (%params directive args '(1)))) (write-char #\~ stream)))
      ((char= c #\P) (%format-plural stream args colon at))
      ((char= c #\*) (%format-goto directive args colon at))
      ((char= c #\[) (%format-conditional directive stream args colon at))
      ((char= c #\{) (%format-iteration directive stream args colon at))
      ((char= c #\() (%format-case directive stream args colon at))
      ((char= c #\?) (%format-indirect stream args at))
      ((char= c #\^) (%format-escape directive args colon))
      (t (error "format: a directive basic format does not have" c)))))

;;; ~A and ~S: print, then pad to MINCOL — on the right, or on the left with
;;; `@` — with at least MINPAD pad characters, added COLINC at a time.
;;; The parameters come first: a `V` takes its value from the arguments
;;; before the directive takes its own.
(defun %format-padded (directive stream args escape)
  (let* ((params (%params directive args '(0 1 0 #\Space)))
         (arg (%next-arg args)))
    (destructuring-bind (mincol colinc minpad padchar) params
      (let ((text (if (and (null arg) (%format-directive-colon directive))
                      "()"
                      (if escape (prin1-to-string arg) (princ-to-string arg)))))
        (%write-padded text stream mincol colinc minpad padchar
                       (%format-directive-at directive))))))

(defun %write-padded (text stream mincol colinc minpad padchar left)
  (let ((pad minpad))
    (do () ((>= (+ (length text) pad) mincol))
      (setq pad (+ pad colinc)))
    (if left (dotimes (i pad) (write-char padchar stream)))
    (write-string text stream)
    (if (not left) (dotimes (i pad) (write-char padchar stream)))))

;;; ~D ~B ~O ~X ~nR: an integer in RADIX, padded on the left to MINCOL. `@`
;;; always shows the sign, `:` puts COMMACHAR between groups of
;;; COMMA-INTERVAL digits. Anything but an integer prints as a bare ~A
;;; would, in the radix and unpadded, as SBCL prints it.
(defun %format-integer (directive stream args radix radix-first)
  (let* ((params (%params directive args
                          (if radix-first '(10 0 #\Space #\, 3) '(0 #\Space #\, 3))))
         (radix (if radix-first (car params) radix))
         (params (if radix-first (cdr params) params))
         (arg (%next-arg args)))
    (destructuring-bind (mincol padchar commachar comma-interval) params
      (if (not (integerp arg))
          (write-string (write-to-string arg :base radix :escape nil :radix nil) stream)
          (let* ((digits (write-to-string (abs arg) :base radix :radix nil))
                 (digits (if (%format-directive-colon directive)
                             (%group-digits digits commachar comma-interval)
                             digits))
                 (text (cond ((< arg 0) (concatenate 'string "-" digits))
                             ((%format-directive-at directive) (concatenate 'string "+" digits))
                             (t digits))))
            (%write-padded text stream mincol 1 0 padchar t))))))

;;; DIGITS with COMMACHAR between groups of INTERVAL, counted from the right.
(defun %group-digits (digits commachar interval)
  (let ((out nil) (count 0))
    (dotimes (k (length digits))
      (let ((i (- (length digits) 1 k)))
        (if (and (> count 0) (zerop (mod count interval))) (push commachar out))
        (push (char digits i) out)
        (setq count (+ count 1))))
    (coerce out 'string)))

;;; ~C: the character itself; `:` the name of one that does not print, and
;;; of a space; `@` as the reader would read it, which is what `prin1`
;;; writes. Since every Latin-1 character has a name (#70), `:` asks whether
;;; the character prints first, as SBCL does: `~:C` of `a` is `a`, not
;;; `LATIN_SMALL_LETTER_A`.
(defun %format-character (char stream colon at)
  (cond (at (prin1 char stream))
        ((and colon (or (not (graphic-char-p char)) (eql char #\Space)) (char-name char))
         (write-string (char-name char) stream))
        (t (write-char char stream))))

;;; ~P: "s" unless the argument is 1. `:` uses the previous argument again;
;;; `@` gives "y" or "ies".
(defun %format-plural (stream args colon at)
  (if colon (%format-back args 1))
  (let ((one (eql (%next-arg args) 1)))
    (write-string (if at (if one "y" "ies") (if one "" "s")) stream)))

;;; ~* skips arguments, ~:* backs up, ~n@* goes to argument N.
(defun %format-goto (directive args colon at)
  (cond (at (setf (%format-args-rest args)
                  (nthcdr (car (%params directive args '(0))) (%format-args-all args))))
        (colon (%format-back args (car (%params directive args '(1)))))
        (t (dotimes (i (car (%params directive args '(1)))) (%next-arg args)))))

(defun %format-back (args n)
  (let ((used (- (length (%format-args-all args)) (%args-left args))))
    (setf (%format-args-rest args) (nthcdr (- used n) (%format-args-all args)))))

;;; ~[…~;…~]: the clause the argument (or the parameter) numbers, or the
;;; default. `~:[false~;true~]` chooses by truth; `~@[…~]` runs its one
;;; clause if the argument is true, and leaves the argument for it to use.
(defun %format-conditional (directive stream args colon at)
  (let ((clauses (%format-directive-clauses directive)))
    (cond
      (colon (%run-items (if (%next-arg args) (second clauses) (first clauses)) stream args))
      (at (if (car (%format-args-rest args))
              (%run-items (first clauses) stream args)
              (%next-arg args)))
      (t (let* ((given (car (%format-directive-params directive)))
                (n (cond ((null given) (%next-arg args))
                         ((eq given :v) (%next-arg args))
                         ((eq given :count) (%args-left args))
                         (t given)))
                (clause (cond ((and (integerp n) (>= n 0) (< n (length clauses))
                                    (not (and (%format-directive-default directive)
                                              (= n (- (length clauses) 1)))))
                               (nth n clauses))
                              ((%format-directive-default directive) (car (last clauses)))
                              (t nil))))
           (%run-items clause stream args))))))

;;; ~{…~}: the clause once per element of a list argument — or, with `@`, of
;;; the remaining arguments. `:` makes each element a sublist of arguments.
;;; ~^ inside ends the iteration when nothing is left to iterate over.
;;;
;;; An empty body, `~{~}`, takes its control from the arguments, before the
;;; list it iterates over (CLHS 22.3.7.4): a string, or a function such as
;;; `formatter` makes, called with the stream and the arguments and
;;; returning the ones it did not use (#81; it looped).
(defun %format-iteration (directive stream args colon at)
  (let* ((body (first (%format-directive-clauses directive)))
         (limit (car (%params directive args '(nil))))
         (control (if body nil (%next-arg args)))
         (body (if (stringp control) (%parse-control control) body))
         (function (if (stringp control) nil control))
         (source (if at nil (%next-arg args)))
         (state (if at args (make-%format-args :all source :rest source)))
         (count 0))
    (catch 'format-iteration
      (let ((%format-escape-tag (if colon 'format-step 'format-iteration))
            (%format-steps state))
        (do () ((or (and limit (>= count limit))
                    (and (null (%format-args-rest state))
                         (not (and (%format-directive-once directive) (= count 0))))))
          (if colon
              (let ((sublist (%next-arg state)))
                (catch 'format-step
                  (%format-iteration-step body function stream
                                          (make-%format-args :all sublist :rest sublist))))
              (%format-iteration-step body function stream state))
          (setq count (+ count 1)))))))

(defun %format-iteration-step (body function stream state)
  (if function
      (setf (%format-args-rest state) (apply function stream (%format-args-rest state)))
      (%run-items body stream state)))

;;; ~^: stop if nothing is left. Inside ~:{ it ends the step, and ~:^ the
;;; whole iteration; elsewhere it ends the innermost ~{, or the whole format.
(defun %format-escape (directive args colon)
  (let ((given (%format-directive-params directive)))
    (if (cond (given (zerop (car (%params directive args '(0)))))
              (colon (zerop (%args-left %format-steps)))
              (t (zerop (%args-left args))))
        (throw (if colon 'format-iteration %format-escape-tag) nil))))

;;; ~(…~): the clause's output in lower case; `:` capitalises every word,
;;; `@` only the first, `:@` upper case.
(defun %format-case (directive stream args colon at)
  (let ((text (with-output-to-string (out)
                (%run-items (first (%format-directive-clauses directive)) out args))))
    (write-string (cond ((and colon at) (string-upcase text))
                        (colon (string-capitalize text))
                        (at (%capitalize-first text))
                        (t (string-downcase text)))
                  stream)))

(defun %capitalize-first (text)
  (let ((out (string-downcase text)))
    (dotimes (i (length out) out)
      (when (alphanumericp (char out i))
        (setf (char out i) (char-upcase (char out i)))
        (return out)))))

;;; ~?: a control string from the arguments, with a list of arguments after
;;; it — or, with `@`, using the arguments that remain.
(defun %format-indirect (stream args at)
  (let ((control (%parse-control (%next-arg args))))
    (if at
        (%run-items control stream args)
        (let ((list (%next-arg args)))
          (%run-items control stream (make-%format-args :all list :rest list))))))

;;; ------------------------------------------------------------------------
;;; format and formatter

;;; Run a control string, or a function made by `formatter`, against
;;; ARGUMENTS on STREAM, and return the arguments left over.
(defun %format-to (stream control arguments)
  (if (not (stringp control))
      (apply control stream arguments)
      (let ((args (make-%format-args :all arguments :rest arguments))
            (items (%parse-control control)))
        ;; A ~^ outside any ~{ ends the whole format.
        (let ((%format-escape-tag 'format-done))
          (catch 'format-done
            (%run-items items stream args)))
        (%format-args-rest args))))

(defun format (destination control &rest arguments)
  (cond ((null destination)
         (with-output-to-string (out) (%format-to out control arguments)))
        ((eq destination t) (%format-to *standard-output* control arguments) nil)
        ((streamp destination) (%format-to destination control arguments) nil)
        (t (error "format: a destination basic format does not have" destination))))

;;; `(formatter control-string)`: a function of a stream and arguments that
;;; formats them and returns the arguments it did not use. Here a closure
;;; over the interpreter (decision 2); #58 would compile it.
(defmacro formatter (control)
  (let ((stream (gensym "STREAM")) (arguments (gensym "ARGUMENTS")))
    `(lambda (,stream &rest ,arguments)
       (%format-to ,stream ,control ,arguments))))

;;; ------------------------------------------------------------------------
;;; Floats (step M, #17)
;;;
;;; Each directive turns the number into text and then pads it on the left
;;; to the width W. Digits come from two places in `floats.lisp`: the
;;; shortest ones that read back as the double (Rust's, decision 5), when no
;;; number of digits is asked for; and an exact rounding of the double's own
;;; bits, halves up, when one is — which is how SBCL rounds: (format nil
;;; "~,1F" 0.25) is "0.3", but 2.675 is really 2.67499999..., so "~,2F"
;;; gives "2.67". A rational argument is made a float first; anything else is
;;; printed as ~A would.

;;; The sign a float directive writes: "-" for a negative number, -0.0
;;; included, and "+" for any other when `@` was given.
(defun %float-sign-text (float at)
  (cond ((%negative-float-p float) "-")
        (at "+")
        (t "")))

;;; TEXT padded on the left to W with PADCHAR — or, if it does not fit and
;;; there is an OVERFLOWCHAR, W of those instead.
(defun %fit-float-text (text stream w overflowchar padchar)
  (cond ((and w overflowchar (> (length text) w))
         (dotimes (i w) (write-char overflowchar stream)))
        (t (%write-padded text stream (or w 0) 1 0 padchar t))))

;;; The float directives' argument, a float — or nil for a non-number, which
;;; the directive has already printed.
(defun %float-argument (directive stream args)
  (let ((arg (%next-arg args)))
    (cond ((floatp arg) arg)
          ((rationalp arg) (float arg))
          (t (%write-padded (princ-to-string arg) stream
                            (or (car (%format-directive-params directive)) 0)
                            1 0 #\Space t)
             nil))))

;;; ~w,d,k,overflowchar,padcharF — positional notation. With D, exactly D
;;; digits after the point; without D or W, every shortest digit; with only
;;; W, as many as fit. K scales by a power of ten. A leading zero goes when
;;; it is all that stops the number fitting: (format nil "~3,2F" 0.5) is
;;; ".50".
(defun %format-fixed (directive stream args)
  (destructuring-bind (w d k overflowchar padchar)
      (%params directive args '(nil nil 0 nil #\Space))
    (let ((x (%float-argument directive stream args)))
      (when x
        (let* ((sign (%float-sign-text x (%format-directive-at directive)))
               (body (%fixed-body x w d k (length sign))))
          (if (and w (> (+ (length sign) (length body)) w)
                   (> (length body) 1) (string= (subseq body 0 2) "0."))
              (setq body (subseq body 1)))
          (%fit-float-text (concatenate 'string sign body) stream w overflowchar padchar))))))

;;; The digits and point of |X| for ~F; SIGN-LENGTH is what the sign will
;;; take of W.
(defun %fixed-body (x w d k sign-length)
  (cond (d (%point-from-right (princ-to-string (%scaled-integer x (+ d k))) d))
        ((null w) (%fixed-shortest (if (zerop k) x (* x (expt 10 k)))))
        (t (let* ((shortest (%fixed-shortest x))
                  (point (position #\. shortest))
                  (room (- w sign-length)))
             (if (<= (length shortest) room)
                 shortest
                 ;; Below one, the zero before the point can go (the caller
                 ;; drops it), so the room is for the point and digits, and
                 ;; one digit at least: (format nil "~1F" 0.5) is ".5". It
                 ;; was "1.", rounded to no digits to make room for a zero
                 ;; that was not going to be printed (#114).
                 (%fixed-body x nil
                              (if (and (> (length shortest) 1)
                                       (string= (subseq shortest 0 2) "0."))
                                  (max 1 (- room 1))
                                  (max 0 (- room point 1)))
                              k sign-length))))))

;;; ~w,d,e,k,overflowchar,padchar,exptcharE — a mantissa and a power of
;;; ten. K digits before the point (default 1); with D, D-K+1 after it, or
;;; for K of zero or less, -K zeros and then D+K digits. The exponent comes
;;; from the shortest digits and is not adjusted when rounding carries, as
;;; SBCL does not: (format nil "~,3E" 9.9996) is "10.000e+0". E pads the
;;; exponent's digits with zeros.
(defun %format-exponential (directive stream args)
  (destructuring-bind (w d e k overflowchar padchar exptchar)
      (%params directive args '(nil nil nil 1 nil #\Space nil))
    (let ((x (%float-argument directive stream args)))
      (when x
        (%fit-float-text (%exponential-text x d e k exptchar (%format-directive-at directive))
                         stream w overflowchar padchar)))))

(defun %exponential-text (x d e k exptchar at)
  (multiple-value-bind (digits exponent)
      (if (zerop x) (values "0" 0) (%float-digits x))
    (let* ((power (- exponent k -1))
           (places (cond ((null d) (max 1 (- (length digits) k)))
                         ((> k 0) (- d k -1))
                         (t d)))
           (mantissa (if (and (null d) (> k 0))
                         (%point-from-right
                          (concatenate 'string digits
                                       (make-string (max 0 (- (+ k places) (length digits)))
                                                    :initial-element #\0))
                          places)
                         (%point-from-right
                          (princ-to-string (%scaled-integer x (- places power)))
                          places)))
           (power-digits (princ-to-string (abs power))))
      (concatenate 'string
                   (%float-sign-text x at)
                   mantissa
                   (string (or exptchar #\e))
                   (if (minusp power) "-" "+")
                   (make-string (max 0 (- (or e 0) (length power-digits))) :initial-element #\0)
                   power-digits))))

;;; ~w,d,e,k,overflowchar,padchar,exptcharG — ~F when the number is of a
;;; size ~F shows well, followed by as many spaces as an exponent would
;;; have taken, and otherwise ~E: CLHS 22.3.3.3's rule, with D's default
;;; counted as SBCL counts it, every digit of the positional form.
(defun %format-general (directive stream args)
  (destructuring-bind (w d e k overflowchar padchar exptchar)
      (%params directive args '(nil nil nil 1 nil #\Space nil))
    (let ((x (%float-argument directive stream args)))
      (when x
        (multiple-value-bind (digits exponent)
            (if (zerop x) (values "0" 0) (%float-digits x))
          (let* ((n (+ exponent 1))
                 (d (or d (max (if (minusp exponent)
                                   (+ (length digits) (- exponent) -1)
                                   (max (length digits) n))
                               (min n 7))))
                 (ee (if e (+ e 2) 4))
                 (dd (- d n))
                 (at (%format-directive-at directive)))
            (if (<= 0 dd d)
                (let* ((sign (%float-sign-text x at))
                       (text (concatenate 'string sign (%fixed-body x nil dd 0 (length sign)))))
                  (%fit-float-text text stream (and w (- w ee)) overflowchar padchar)
                  (dotimes (i ee) (write-char #\Space stream)))
                (%fit-float-text (%exponential-text x d e k exptchar at)
                                 stream w overflowchar padchar))))))))

;;; ~d,n,w,padchar$ — money: D digits after the point (default 2), at least
;;; N before it (default 1), padded on the left to W. `@` always shows the
;;; sign; `:` puts it before the padding.
(defun %format-money (directive stream args)
  (destructuring-bind (d n w padchar)
      (%params directive args '(2 1 0 #\Space))
    (let ((x (%float-argument directive stream args)))
      (when x
        (let* ((sign (%float-sign-text x (%format-directive-at directive)))
               (body (%point-from-right (princ-to-string (%scaled-integer x d)) d))
               (whole (position #\. body))
               (body (if (< whole n)
                         (concatenate 'string (make-string (- n whole) :initial-element #\0) body)
                         body)))
          (if (%format-directive-colon directive)
              (progn (write-string sign stream)
                     (%write-padded body stream (- w (length sign)) 1 0 padchar t))
              (%write-padded (concatenate 'string sign body) stream w 1 0 padchar t)))))))
