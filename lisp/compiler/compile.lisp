;;;; The compiler: Lisp forms to a list of instructions.
;;;;
;;;; Tier 0 only (rule 5). This file is read by the phase 0 interpreter, and a
;;;; compiler that needed features its own output provides could never be
;;;; bootstrapped.
;;;;
;;;; What is here and what is not. This emits instructions as lists —
;;;; `(LREF 0 0)`, `(JMP-NIL ELSE)`, `(:LABEL ELSE)` — and hands them to
;;;; `%assemble`, which resolves the labels, encodes the operands, verifies
;;;; the result and computes its stack depth. None of that is in Lisp, because
;;;; the opcode numbers and operand widths would then exist in two places with
;;;; nothing checking they agree. See issue #31 and book chapter 24.
;;;;
;;;; So everything here is a decision about the *language*: what `if` compiles
;;;; to, which calls are in tail position, when a primitive may be called
;;;; directly. That division is rule 1 applied to the compiler itself.

;;; The constants of the function being compiled.
;;;
;;; A global rather than an argument threaded through every function, because
;;; the compiler compiles one function at a time and the threading would be
;;; most of the code. It is reset at each entry point, which is the whole of
;;; the discipline that makes a global safe here.
;;;
;;; Held reversed and counted, so adding one is a `cons` rather than a walk to
;;; the end.

(setq %constants nil)
(setq %constant-count 0)

(defun %reset-constants ()
  (setq %constants nil)
  (setq %constant-count 0))

(defun %find-constant (value remaining index)
  (cond ((null remaining) nil)
        ((eq (car remaining) value) index)
        (t (%find-constant value (cdr remaining) (- index 1)))))

;;; The index of VALUE, adding it if it is not there.
;;;
;;; Matched with `eq`, so two occurrences of a symbol share a slot and two
;;; equal strings do not. That is the right way round: a symbol is interned
;;; and a literal string is a separate object whose identity a program can
;;; observe (chapter 26 and issue #26).
(defun %constant-index (value)
  (let ((found (%find-constant value %constants (- %constant-count 1))))
    (if found
        found
        (progn
          (setq %constants (cons value %constants))
          (setq %constant-count (+ %constant-count 1))
          (- %constant-count 1)))))

(defun %constants-vector ()
  (apply #'vector (reverse %constants)))

;;; Does this integer fit in a FIX instruction's signed 16-bit operand?
;;;
;;; If it does it costs no constant slot, which matters because small integers
;;; are most of what a program mentions. The bound is the instruction's, and
;;; `%assemble` would refuse anything larger rather than wrap it.
(defun %small-integer-p (n)
  (and (> n -32769) (< n 32768)))

;;; Compiling a value that is not a form to evaluate: a quoted object, or a
;;; self-evaluating one. The three with their own opcodes get them.
(defun %compile-value (value)
  (cond ((null value) (list (list 'NIL)))
        ((eq value t) (list (list 'T)))
        ((and (integerp value) (%small-integer-p value))
         (list (list 'FIX value)))
        (t (list (list 'CONST (%constant-index value))))))

;;; `(if test then else)`
;;;
;;; There is no branch instruction that yields a value, so `if` is a jump over
;;; the branch not taken — which is the whole of control flow down here, and
;;; why `when`, `cond`, `and`, `or` and every loop are macros that reach this
;;; same pair of instructions (chapter 25).
;;;
;;; Both arms must leave exactly one value, or the verifier refuses the
;;; function: the paths meet at DONE and have to agree about the stack.
(defun %compile-if (form)
  (let ((else (gensym))
        (done (gensym)))
    (append (%compile (second form))
            (list (list 'JMP-NIL else))
            (%compile (third form))
            (list (list 'JMP done))
            (list (list ':label else))
            (%compile (nth 3 form))
            (list (list ':label done)))))

(defun %compile-arguments (arguments)
  (if (endp arguments)
      nil
      (append (%compile (car arguments))
              (%compile-arguments (cdr arguments)))))

;;; `(f a b)` — the callee first, then the arguments, then CALL.
;;;
;;; GFREF reads the symbol's function cell at run time, so redefining `f`
;;; takes effect. That is right for a user's function and wasteful for `+`,
;;; which cannot be redefined without undefined consequences (CLHS 11.1.2.1.2)
;;; and could compile to a PRIM instruction instead. Not yet: the uniform
;;; version is the one to get working first, and the inlining decision
;;; deserves its own change and its own explanation.
(defun %compile-call (form)
  (append (list (list 'GFREF (%constant-index (car form))))
          (%compile-arguments (cdr form))
          (list (list 'CALL (length (cdr form))))))

(defun %compile-cons (form)
  (let ((head (car form)))
    (cond ((eq head 'quote) (%compile-value (second form)))
          ((eq head 'if) (%compile-if form))
          (t (%compile-call form)))))

;;; One form to a list of instructions leaving exactly one value.
(defun %compile (form)
  (cond ((null form) (list (list 'NIL)))
        ((eq form t) (list (list 'T)))
        ((symbolp form) (list (list 'GREF (%constant-index form))))
        ((consp form) (%compile-cons form))
        (t (%compile-value form))))

;;; Compile FORM into a function of no arguments.
;;;
;;; The entry point, and the only place the constants are reset — which is
;;; what keeps the global above honest.
(defun compile-form (form)
  (%reset-constants)
  (let ((code (append (%compile form) (list (list 'RET)))))
    (%assemble code (%constants-vector) nil 'compiled-form)))
