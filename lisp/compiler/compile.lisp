;;;; The compiler: Lisp forms to a list of instructions.
;;;;
;;;; Tier 0 only (rule 5). This file is read by the phase 0 interpreter, and
;;;; a compiler that needed features its own output provides could never be
;;;; bootstrapped.
;;;;
;;;; What is here and what is not. This emits instructions as lists —
;;;; `(LREF 0 0)`, `(JMP-NIL ELSE)`, `(:LABEL ELSE)` — and hands them to
;;;; `%assemble`, which resolves the labels, encodes the operands, verifies
;;;; the result and computes its stack depth. None of that is in Lisp,
;;;; because the opcode numbers and operand widths would then exist in two
;;;; places with nothing checking they agree; see issue #31 and chapter 24.
;;;;
;;;; So everything in this file is a decision about the *language*. What `if`
;;;; compiles to. Which calls are in tail position. When a primitive may be
;;;; called directly rather than through its function cell. That division is
;;;; rule 1, applied to the compiler itself.

;;; Emitting
;;;
;;; A compiler function takes a form and returns a list of instructions. They
;;; are appended, so the list grows at the end; `append` is O(n) and this will
;;; want a better representation long before the compiler compiles itself.
;;; Said rather than hidden, as everywhere else.

(defun %instr (name &rest operands)
  (cons name operands))

;;; Constants
;;;
;;; Small integers get `FIX`, which carries the value in the instruction and
;;; costs no constant slot. `nil` and `t` get their own opcodes for the same
;;; reason: they are most of what a program mentions.
;;;
;;; Everything else goes in the constants vector, and the compiler has to
;;; remember what it has put there — which is what the `constants` argument
;;; threaded through everything below is for.

(defun %self-evaluating-p (form)
  (cond ((null form) t)
        ((eq form t) t)
        ((integerp form) t)
        ((characterp form) t)
        ((stringp form) t)
        (t nil)))
