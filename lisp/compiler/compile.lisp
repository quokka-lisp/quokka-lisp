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

;;; The compile-time environment
;;;
;;; A list of scopes, each a list of names, innermost first. It mirrors the
;;; chain of `Environment` objects the machine will build, and `LREF d i`
;;; counts *links* — so the two have to agree link for link.
;;;
;;; That is the trap book chapter 26 warns about, and it is worth restating
;;; where the counting happens: a depth that is off by one does not crash, it
;;; reads a different variable. Every place below that adds a scope to this
;;; list is a place the machine must also push one, and the reverse.
;;;
;;; Not handled yet: `flet`, which binds in the *function* namespace. Its
;;; scope is still a link in the chain, so the compiler will have to count it
;;; while a variable lookup steps over it.

(defun %position (item remaining index)
  (cond ((null remaining) nil)
        ((eq (car remaining) item) index)
        (t (%position item (cdr remaining) (+ index 1)))))

;;; A scope is `(tag name ...)`, where the tag is `:variable` or `:function`.
;;;
;;; The tag matters because `depth` counts **every** link, whatever namespace
;;; it binds in — an `flet` scope is stepped over by a variable lookup and is
;;; still a link. Chapter 26 works the case through; this is where the
;;; counting has to be right, and `LREF` and `FREF` check the namespace on
;;; arrival so that getting it wrong is an error rather than a wrong answer.
(defun %scope (tag names) (cons tag names))

(defun %lookup (name env depth tag)
  (if (null env)
      nil
      (let ((scope (car env)))
        (let ((index (if (eq (car scope) tag)
                         (%position name (cdr scope) 0)
                         nil)))
          (if index
              (cons depth index)
              (%lookup name (cdr env) (+ depth 1) tag))))))

;;; `(if test then else)`
;;;
;;; There is no branch instruction that yields a value, so `if` is a jump over
;;; the branch not taken — which is the whole of control flow down here, and
;;; why `when`, `cond`, `and`, `or` and every loop are macros that reach this
;;; same pair of instructions (chapter 25).
;;;
;;; Both arms must leave exactly one value, or the verifier refuses the
;;; function: the paths meet at DONE and have to agree about the stack.
(defun %compile-if (form env)
  (let ((else (gensym))
        (done (gensym)))
    (append (%compile (second form) env)
            (list (list 'JMP-NIL else))
            (%compile (third form) env)
            (list (list 'JMP done))
            (list (list ':label else))
            (%compile (nth 3 form) env)
            (list (list ':label done)))))

;;; A body: every form but the last is evaluated for effect, so its value is
;;; popped. An empty body is `nil`, which is what `(progn)` means.
(defun %compile-body (forms env)
  (cond ((null forms) (list (list 'NIL)))
        ((null (cdr forms)) (%compile (car forms) env))
        (t (append (%compile (car forms) env)
                   (list (list 'POP))
                   (%compile-body (cdr forms) env)))))

(defun %binding-name (binding)
  (if (consp binding) (car binding) binding))

(defun %binding-init (binding)
  (if (consp binding) (second binding) nil))

(defun %binding-names (bindings)
  (if (endp bindings)
      nil
      (cons (%binding-name (car bindings))
            (%binding-names (cdr bindings)))))

(defun %binding-inits (bindings)
  (if (endp bindings)
      nil
      (cons (%binding-init (car bindings))
            (%binding-inits (cdr bindings)))))

;;; `(let ((a 1) (b 2)) ...)`
;;;
;;; Every init is evaluated in the *outer* scope — which is what `let` means,
;;; and which falls out here rather than being arranged: the inits are
;;; compiled before `BIND` pushes anything. `let*` would push a scope per
;;; binding, and is not here yet.
;;;
;;; `BIND` takes the names from a constant vector it does not look at for
;;; anything but their count. They are carried for the debugger, which is the
;;; only reason a compiled scope knows what its variables are called.
(defun %compile-let (form env)
  (let ((names (%binding-names (second form)))
        (inits (%binding-inits (second form))))
    (append (%compile-arguments inits env)
            (list (list 'BIND (%constant-index (apply #'vector names))))
            (%compile-body (cddr form) (cons (%scope ':variable names) env))
            (list (list 'UNBIND 1)))))

;;; `(setq x v)` — the value stays on the stack, because `setq` returns it.
;;; `LSET` and `GSET` pop and push, so that is what they already do.
(defun %compile-setq (form env)
  (let ((place (%lookup (second form) env 0 ':variable)))
    (append (%compile (third form) env)
            (if place
                (list (list 'LSET (car place) (cdr place)))
                (list (list 'GSET (%constant-index (second form))))))))

;;; `(lambda (x) ...)`
;;;
;;; A whole function of its own, compiled into the enclosing one's constants,
;;; and `CLOSURE` captures the current scope chain at run time.
;;;
;;; The constants being accumulated have to be saved and put back, because
;;; they live in a global and this is the one place that is reentrant. A
;;; global is cheaper than threading an argument through every function here;
;;; the cost is exactly this, and it is paid in one place.
;;;
;;; Only simple parameter lists for now. `&optional` and `&key` need their
;;; default forms turned into a `let*` around the body — which the interpreter
;;; does in `lambda_list::rewrite` and the compiler will have to do for
;;; itself.
(defun %compile-lambda (form env)
  (let ((params (second form)))
    (if (member '&optional params)
        (error "the compiler cannot do &optional yet")
        (if (member '&rest params)
            (error "the compiler cannot do &rest yet")
            (if (member '&key params)
                (error "the compiler cannot do &key yet")
                (%compile-simple-lambda params (cddr form) env))))))

(defun %compile-simple-lambda (params body env)
  (let ((outer %constants)
        (outer-count %constant-count))
    (%reset-constants)
    (let ((code (append (%compile-body body (cons (%scope ':variable params) env))
                        (list (list 'RET)))))
      (let ((compiled (%assemble code (%constants-vector) params 'lambda)))
        (setq %constants outer)
        (setq %constant-count outer-count)
        (list (list 'CLOSURE (%constant-index compiled)))))))

(defun %compile-arguments (arguments env)
  (if (endp arguments)
      nil
      (append (%compile (car arguments) env)
              (%compile-arguments (cdr arguments) env))))

;;; `(f a b)` — the callee first, then the arguments, then CALL.
;;;
;;; GFREF reads the symbol's function cell at run time, so redefining `f`
;;; takes effect. That is right for a user's function and wasteful for `+`,
;;; which cannot be redefined without undefined consequences (CLHS 11.1.2.1.2)
;;; and could compile to a PRIM instruction instead. Not yet: the uniform
;;; version is the one to get working first, and the inlining decision
;;; deserves its own change and its own explanation.
(defun %compile-callee (head env)
  (cond ((symbolp head)
         ;; A local function first, which is what makes this a Lisp-2 worth
         ;; having: `(flet ((list (x) x)) (list 1))` calls the local one and
         ;; the variable `list` is untouched.
         (let ((place (%lookup head env 0 ':function)))
           (if place
               (list (list 'FREF (car place) (cdr place)))
               (list (list 'GFREF (%constant-index head))))))
        ;; `((lambda (x) ...) 1)` — a lambda expression in operator position,
        ;; which is a call like any other once the closure is on the stack.
        ((and (consp head) (eq (car head) 'lambda)) (%compile-lambda head env))
        (t (error "a call wants a symbol or a lambda expression as its operator"))))

(defun %compile-call (form env)
  (append (%compile-callee (car form) env)
          (%compile-arguments (cdr form) env)
          (list (list 'CALL (length (cdr form))))))

;;; `(flet ((f (x) ...)) ...)`
;;;
;;; The closures are made *before* the scope exists, in the enclosing
;;; environment — which is exactly what distinguishes `flet` from `labels`:
;;; an `flet` function cannot see itself or its siblings.
(defun %compile-local-functions (definitions env)
  (if (endp definitions)
      nil
      (append (%compile-lambda (cons 'lambda (cdr (car definitions))) env)
              (%compile-local-functions (cdr definitions) env))))

(defun %compile-flet (form env)
  (let ((names (%binding-names (second form))))
    (append (%compile-local-functions (second form) env)
            (list (list 'BINDF (%constant-index (apply #'vector names))))
            (%compile-body (cddr form) (cons (%scope ':function names) env))
            (list (list 'UNBIND 1)))))

;;; `(labels ((f (x) ... (f ...))) ...)`
;;;
;;; The functions can call themselves and each other, so the closures have to
;;; capture a scope that already exists — which means the scope is made first,
;;; with nothing in it, and the closures are stored into it afterwards.
;;;
;;; **That is the instruction the specification did not have.** `docs/bytecode.md`
;;; had `LSET` for a variable and `GSET` for a global and nothing that writes
;;; the function namespace, because `flet` does not need one and writing the VM
;;; never reached a case that did. `FSET` is the thirty-seventh opcode, and it
;;; exists because the compiler asked for it.
(defun %nils (n)
  (if (= n 0) nil (cons (list 'NIL) (%nils (- n 1)))))

(defun %store-local-functions (definitions env index)
  (if (endp definitions)
      nil
      (append (%compile-lambda (cons 'lambda (cdr (car definitions))) env)
              (list (list 'FSET 0 index))
              (list (list 'POP))
              (%store-local-functions (cdr definitions) env (+ index 1)))))

(defun %compile-labels (form env)
  (let ((names (%binding-names (second form))))
    (let ((inner (cons (%scope ':function names) env)))
      (append (%nils (length names))
              (list (list 'BINDF (%constant-index (apply #'vector names))))
              (%store-local-functions (second form) inner 0)
              (%compile-body (cddr form) inner)
              (list (list 'UNBIND 1))))))

;;; A list form: a special operator, a macro call, or a function call.
;;;
;;; The order matters and is the standard's. A special operator is not
;;; shadowable by a macro, and a macro call is expanded before anything looks
;;; at it as a call — so `when` never reaches `%compile-call`, and neither
;;; does anything a user defines with `defmacro`.
(defun %compile-cons (form env)
  (let ((head (car form)))
    (cond ((eq head 'quote) (%compile-value (second form)))
          ((eq head 'if) (%compile-if form env))
          ((eq head 'progn) (%compile-body (cdr form) env))
          ((eq head 'let) (%compile-let form env))
          ((eq head 'setq) (%compile-setq form env))
          ((eq head 'lambda) (%compile-lambda form env))
          ((eq head 'flet) (%compile-flet form env))
          ((eq head 'labels) (%compile-labels form env))
          (t (multiple-value-bind (expansion expanded) (macroexpand-1 form)
               (if expanded
                   (%compile expansion env)
                   (%compile-call form env)))))))

;;; One form to a list of instructions leaving exactly one value.
;;;
;;; A symbol is a variable: found in the compile-time environment it is a pair
;;; of small numbers, and not found it is a global read through its symbol at
;;; run time. Which is chapter 26's distinction, and the reason for it — a
;;; lexical binding is created and destroyed by code the compiler can see in
;;; full, and a global is reachable by anything at any time.
(defun %compile (form env)
  (cond ((null form) (list (list 'NIL)))
        ((eq form t) (list (list 'T)))
        ((symbolp form)
         (let ((place (%lookup form env 0 ':variable)))
           (if place
               (list (list 'LREF (car place) (cdr place)))
               (list (list 'GREF (%constant-index form))))))
        ((consp form) (%compile-cons form env))
        (t (%compile-value form))))

;;; Compile FORM into a function of no arguments.
;;;
;;; The entry point, and the only place the constants are reset from nothing —
;;; `%compile-simple-lambda` saves and restores them instead.
(defun compile-form (form)
  (%reset-constants)
  (let ((code (append (%compile form nil) (list (list 'RET)))))
    (%assemble code (%constants-vector) nil 'compiled-form)))
