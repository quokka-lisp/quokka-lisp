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

;;; What is open around the form being compiled, innermost first: `:record`
;;; for a form whose unwind record is on the stack while its body runs,
;;; `:function` for the edge of a nested function, and an entry per
;;; `tagbody`. A `go` reads it to decide whether it may be a one-step
;;; `JMP-TAG` — only if its own tagbody is the innermost thing open (#37).
;;;
;;; Not part of the environment, because the environment mirrors the run-time
;;; chain of scopes link for link, and most of these leave no scope behind.
(setq %open nil)

;;; Special variables (#40): the symbols `defvar` and `defparameter` have
;;; proclaimed. Every binding of one is dynamic, wherever it appears — a `let`,
;;; a `let*`, a lambda parameter — so the compiler has to know which they are
;;; before it compiles a binding.
;;;
;;; A table here rather than a mark on the symbol, because the compiler is the
;;; only thing that asks, and a table costs no Rust (#39: 1,915 lines left for
;;; phases 2 to 6). Proclaiming happens when `defvar`'s *expansion* is made as
;;; well as when it runs, so that a binding compiled later in the same
;;; top-level form is already special — what CL's compile-time processing of
;;; `defvar` is for.
(setq %special-variables (make-hash-table))

(defun %proclaim-special (symbol)
  (setf (gethash symbol %special-variables) t)
  symbol)

(defun %special-variable-p (symbol)
  (gethash symbol %special-variables))

;;; Constants (#63, part 4): `t`, `nil`, keywords, and the names
;;; `defconstant` proclaims, kept in a table as the specials are. A constant
;;; may not be bound or assigned: `let`, `let*`, a lambda parameter, `setq`
;;; and `symbol-macrolet` of one are refused at compile time with a
;;; `program-error`, as SBCL refuses them.
(setq %constant-variables (make-hash-table))

(defun %proclaim-constant (symbol)
  (setf (gethash symbol %constant-variables) t)
  symbol)

(defun %constant-name-p (name)
  (or (eq name t) (null name) (keywordp name) (gethash name %constant-variables)))

(defun %refuse-constants (names)
  (cond ((endp names) nil)
        ((and (symbolp (car names)) (%constant-name-p (car names)))
         (error 'program-error))
        (t (%refuse-constants (cdr names)))))

;;; Declarations (#62, part 5). `(declare ...)` at the start of a body is
;;; advice the compiler may ignore (CLHS 3.3.1) — `ignore`, `type`,
;;; `optimize` and the rest — except `special`, which changes what a name
;;; means. A name declared special by a binding form is bound dynamically
;;; there, as a proclaimed one always is; and every reference to a name
;;; declared special in a body reaches its value cell, even where a lexical
;;; binding of it is in scope (`locally`). The prelude makes `declare` a macro
;;; that expands to nil, so a declaration anywhere evaluates harmlessly; only
;;; these few places read what one says.
;;;
;;; DECLARED is the list of names a form declares special. Passed along
;;; rather than kept in a global, because it belongs to one form's bindings
;;; and not to anything compiled inside them.

;;; The declarations at the start of BODY, each `(special a b)`, `(ignore x)`
;;; …; a documentation string before them is stepped over.
(defun %declarations (body)
  (cond ((and (stringp (car body)) (cdr body)) (%declarations (cdr body)))
        ((and (consp (car body)) (eq (car (car body)) 'declare))
         (append (cdr (car body)) (%declarations (cdr body))))
        (t nil)))

;;; The names BODY's declarations declare special.
(defun %declared-specials (body)
  (%special-names (%declarations body)))

(defun %special-names (specifiers)
  (cond ((endp specifiers) nil)
        ((and (consp (car specifiers)) (eq (car (car specifiers)) 'special))
         (append (cdr (car specifiers)) (%special-names (cdr specifiers))))
        (t (%special-names (cdr specifiers)))))

;;; A body's `notinline` declarations, as a compile-only scope like the
;;; `special` one: a compiler macro is not applied to a call of a function
;;; declared `notinline` where the call is (CLHS 3.2.2.1.3; ansi-test
;;; define-compiler-macro.7, #80). `%lookup` steps over the scope.
(defun %notinline-scope (body env)
  (let ((names (%notinline-names (%declarations body))))
    (if names (cons (cons ':notinline names) env) env)))

(defun %notinline-names (specifiers)
  (cond ((endp specifiers) nil)
        ((and (consp (car specifiers)) (eq (car (car specifiers)) 'notinline))
         (append (cdr (car specifiers)) (%notinline-names (cdr specifiers))))
        (t (%notinline-names (cdr specifiers)))))

(defun %notinline-p (name env)
  (cond ((null env) nil)
        ((and (eq (car (car env)) ':notinline) (member name (cdr (car env)))) t)
        (t (%notinline-p name (cdr env)))))

;;; Is NAME special in a form that declares DECLARED special?
(defun %special-here-p (name declared)
  (or (%special-variable-p name) (member name declared)))

;;; ENV for a body whose declarations make DECLARED special: a scope that
;;; exists only while compiling, like `symbol-macrolet`'s, which `%lookup`
;;; stops at for those names — so they are read from their value cells — and
;;; steps over for anything else, without counting it.
(defun %special-scope (declared env)
  (if declared (cons (cons ':special declared) env) env))

;;; The names among NAMES that are special.
(defun %specials-among (names declared)
  (cond ((endp names) nil)
        ((%special-here-p (car names) declared)
         (cons (car names) (%specials-among (cdr names) declared)))
        (t (%specials-among (cdr names) declared))))

;;; NAMES with every special replaced by a fresh symbol. The value is still
;;; bound in the lexical scope — that is where `BIND` and a call put it — but
;;; under a name nothing can refer to, so every reference to the special in
;;; the body reaches its value cell instead, as it must.
(defun %rename-specials (names declared)
  (cond ((endp names) nil)
        ((%special-here-p (car names) declared)
         (cons (gensym) (%rename-specials (cdr names) declared)))
        (t (cons (car names) (%rename-specials (cdr names) declared)))))

;;; For each special in NAMES, copy its value out of the scope just bound —
;;; where RENAMED says it is — and bind it dynamically.
(defun %emit-dynamic-binds (names renamed index declared)
  (if (endp names)
      nil
      (progn (if (%special-here-p (car names) declared)
                 (progn (%emit (list 'LREF 0 index))
                        (%emit (list 'DBIND (%constant-index (car names))))))
             (%emit-dynamic-binds (cdr names) (cdr renamed) (+ index 1) declared))))

;;; Leave N dynamic bindings: each `UNMARK` drops one record, keeps the value
;;; above it, and puts the old value of its symbol back.
(defun %emit-unmarks (n)
  (if (> n 0)
      (progn (%emit (list 'UNMARK))
             (%emit-unmarks (- n 1)))))

;;; Compile BODY with dynamic bindings open for the specials among NAMES,
;;; whose values are in the innermost scope under RENAMED.
;;;
;;; Not in tail position: the bindings must be undone after the body, so a
;;; call in it cannot reuse the frame. And open as a record for `go`, so a
;;; `go` out of it uses the unwinder, which restores them.
(defun %compile-with-dynamic-binds (names renamed body env declared)
  (let ((count (length (%specials-among names declared))))
    (%emit-dynamic-binds names renamed 0 declared)
    (%with-open ':record (lambda () (%compile-body body env nil)))
    (%emit-unmarks count)))

;;; Compile with ENTRY open, by calling THUNK. The save and restore are by
;;; hand because tier 0 has no special variables to bind.
(defun %with-open (entry thunk)
  (let ((outer %open))
    (setq %open (cons entry %open))
    (funcall thunk)
    (setq %open outer)))

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

;;; Emitting
;;;
;;; Instructions are accumulated **reversed** in a global and turned the right
;;; way round once, at the end. Adding one is a `cons`.
;;;
;;; The first version had each compiler function return a list and joined them
;;; with `append`, which reads better and is quadratic: `append` copies its
;;; first argument, so building n instructions copies n, then n-1, then n-2.
;;; For a hundred-instruction function that is thousands of conses of pure
;;; garbage, and the compiler is the one thing here that allocates enough for
;;; it to matter (issue #32).
;;;
;;; Like `%constants`, this is a global with one rule: every entry point
;;; resets it, and the one reentrant place — a nested lambda — saves and
;;; restores it.

(setq %code nil)

(defun %emit (instruction)
  (setq %code (cons instruction %code)))

(defun %emit-label (name)
  (%emit (list ':label name)))

;;; Does this integer fit in a FIX instruction's signed 16-bit operand?
;;;
;;; If it does it costs no constant slot, which matters because small integers
;;; are most of what a program mentions. The bound is the instruction's, and
;;; `%assemble` would refuse anything larger rather than wrap it.
(defun %small-integer-p (n)
  (and (> n -32769) (< n 32768)))

;;; A value that is not a form to evaluate: a quoted object, or a
;;; self-evaluating one. The three with their own opcodes get them.
(defun %compile-value (value)
  (cond ((null value) (%emit (list 'NIL)))
        ((eq value t) (%emit (list 'T)))
        ((and (integerp value) (%small-integer-p value))
         (%emit (list 'FIX value)))
        (t (%emit (list 'CONST (%constant-index value))))))

;;; A scope is `(tag name ...)`, where the tag is `:variable` or `:function`.
;;;
;;; The tag matters because `depth` counts **every** link, whatever namespace
;;; it binds in — an `flet` scope is stepped over by a variable lookup and is
;;; still a link. Chapter 26 works the case through; this is where the
;;; counting has to be right, and `LREF` and `FREF` check the namespace on
;;; arrival so that getting it wrong is an error rather than a wrong answer.
(defun %scope (tag names) (cons tag names))

(defun %position (item remaining index)
  (cond ((null remaining) nil)
        ((eq (car remaining) item) index)
        ;; A `go` tag may be an integer, and two bignums read separately
        ;; are `eql` without being `eq` (CLHS `tagbody`; ansi-test
        ;; tagbody.15, #80). Names are symbols, so this costs a type test.
        ((and (integerp item) (integerp (car remaining)) (= (car remaining) item)) index)
        (t (%position item (cdr remaining) (+ index 1)))))

(defun %lookup (name env depth tag)
  (cond
    ((null env) nil)
    ;; A `symbol-macrolet` scope exists only while compiling: nothing is
    ;; pushed for it at run time, so it is not a link and is not counted.
    ((eq (car (car env)) ':symbol-macro) (%lookup name (cdr env) depth tag))
    ((eq (car (car env)) ':notinline) (%lookup name (cdr env) depth tag))
    ;; Nor is a `special` declaration's (#62): a variable it names is not
    ;; lexical here, whatever binds it further out.
    ((eq (car (car env)) ':special)
     (if (and (eq tag ':variable) (member name (cdr (car env))))
         nil
         (%lookup name (cdr env) depth tag)))
    (t
      (let ((scope (car env)))
        (let ((index (if (eq (car scope) tag)
                         (%position name (cdr scope) 0)
                         nil)))
          (if index
              (cons depth index)
              (%lookup name (cdr env) (+ depth 1) tag)))))))

;;; `(if test then else)`
;;;
;;; There is no branch instruction that yields a value, so `if` is a jump over
;;; the branch not taken — which is the whole of control flow down here, and
;;; why `when`, `cond`, `and`, `or` and every loop are macros that reach this
;;; same pair of instructions (chapter 25).
;;;
;;; Both arms must leave exactly one value, or the verifier refuses the
;;; function: the paths meet at DONE and have to agree about the stack.
(defun %compile-if (form env tail)
  (let ((else (gensym))
        (done (gensym)))
    (%compile (second form) env nil)
    (%emit (list 'JMP-NIL else))
    (%compile (third form) env tail)
    (%emit (list 'JMP done))
    (%emit-label else)
    (%compile (nth 3 form) env tail)
    (%emit-label done)))

;;; A body: every form but the last is evaluated for effect, so its value is
;;; popped. An empty body is `nil`, which is what `(progn)` means.
(defun %compile-body (forms env tail)
  (cond ((null forms) (%emit (list 'NIL)))
        ((null (cdr forms)) (%compile (car forms) env tail))
        (t (%compile (car forms) env nil)
           (%emit (list 'POP))
           (%compile-body (cdr forms) env tail))))

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
;;; compiled before `BIND` pushes anything.
;;;
;;; `BIND` takes the names from a constant vector it does not look at for
;;; anything but their count. They are carried for the debugger, which is the
;;; only reason a compiled scope knows what its variables are called.
(defun %compile-let (form env tail)
  (let ((names (%binding-names (second form)))
        (declared (%declared-specials (cddr form))))
    (%refuse-constants names)
    (%compile-arguments (%binding-inits (second form)) env)
    (if (%specials-among names declared)
        ;; Every init is evaluated before anything is bound, special or not —
        ;; which is what `let` means. So the values are bound lexically first,
        ;; the specials under hidden names, and then copied into their symbols.
        (let ((renamed (%rename-specials names declared)))
          (%emit (list 'BIND (%constant-index (apply #'vector renamed))))
          (%compile-with-dynamic-binds names renamed (cddr form)
                                       (%special-scope declared
                                                       (cons (%scope ':variable renamed) env))
                                       declared))
        (progn (%emit (list 'BIND (%constant-index (apply #'vector names))))
               (%compile-body (cddr form)
                              (%special-scope declared (cons (%scope ':variable names) env))
                              tail)))
    (%emit (list 'UNBIND 1))))

;;; `(let* ((a 1) (b (+ a 1))) ...)`
;;;
;;; A scope per binding, because each init can see the ones before it. That is
;;; the whole difference from `let`, and it is why `let` exists at all.
;;;
;;; It is here because `%rewrite-lambda-list` emits one: a lambda list with
;;; `&optional` or `&key` becomes a simple list plus a `let*` that turns each
;;; unsupplied marker into its default. The compiler could not do `&optional`
;;; without doing `let*` first, which is a dependency the specification did
;;; not mention and the work found.
(defun %compile-let* (bindings body env tail)
  (%compile-let*-declaring bindings body env tail (%declared-specials body)))

(defun %compile-let*-declaring (bindings body env tail declared)
  (if (endp bindings)
      (%compile-body body (%special-scope declared env) tail)
      (let ((name (%binding-name (car bindings))))
        (%refuse-constants (list name))
        (%compile (%binding-init (car bindings)) env nil)
        (if (%special-here-p name declared)
            ;; One special at a time, as `let*` binds everything: the init has
            ;; seen every binding before it, and every binding after it sees
            ;; this one.
            (progn (%emit (list 'DBIND (%constant-index name)))
                   (%with-open ':record
                               (lambda () (%compile-let*-declaring (cdr bindings) body env nil
                                                                    declared)))
                   (%emit (list 'UNMARK)))
            (progn (%emit (list 'BIND (%constant-index (vector name))))
                   (%compile-let*-declaring (cdr bindings)
                                            body
                                            (cons (%scope ':variable (list name)) env)
                                            tail
                                            declared)
                   (%emit (list 'UNBIND 1)))))))

;;; `(setq x v)` — the value stays on the stack, because `setq` returns it.
;;; `LSET` and `GSET` pop and push, so that is what they already do.
(defun %compile-setq (form env)
  (if (null (cdr form))
      (%emit (list 'NIL))
      (%compile-setq-pairs (cdr form) env)))

;;; Every pair, in order, and the value of the last. Until step E (#38) this
;;; compiled only the first pair, so `(setq a 1 b 2)` returned 1 and never set
;;; `b` — and nothing noticed, because the only tests of `setq` with more than
;;; one pair were in a differential file that had never been run compiled.
(defun %compile-setq-pairs (pairs env)
  (%refuse-constants (list (car pairs)))
  (let ((macro (%symbol-macro (car pairs) env)))
    ;; `setq` of a symbol macro is `setf` of what it stands for (CLHS
    ;; `setq`), so `(setq x 5)` with `x` meaning `(car cell)` writes the car.
    (if macro
        (%compile (list 'setf (cdr macro) (second pairs)) env nil)
        (let ((place (%lookup (car pairs) env 0 ':variable)))
          (%compile (second pairs) env nil)
          (if place
              (%emit (list 'LSET (car place) (cdr place)))
              (%emit (list 'GSET (%constant-index (car pairs)))))))
    (if (cddr pairs)
        (progn (%emit (list 'POP))
               (%compile-setq-pairs (cddr pairs) env)))))

;;; `(lambda (x) ...)`
;;;
;;; A whole function of its own, compiled into the enclosing one's constants,
;;; and `CLOSURE` captures the current scope chain at run time.
;;;
;;; The instructions and the constants being accumulated both have to be saved
;;; and put back, because they live in globals and this is the one place that
;;; is reentrant. Globals are cheaper than threading two arguments through
;;; every function here; the cost is exactly this, and it is paid once.
;;;
;;; `&optional`, `&key` and `&rest` are not handled here and must not be.
;;; `%rewrite-lambda-list` turns the lambda list into a simple one plus a
;;; `let*` around the body — the same rewrite the interpreter uses, because
;;; writing it again would be two implementations of `&key` that have to
;;; agree. It also hands back the names the simple list binds, in the order
;;; the binder binds them, so the compile-time scope matches the machine's
;;; slot for slot rather than by a second reading of the same syntax.
(defun %compile-lambda (form env)
  (let ((rewritten (%rewrite-lambda-list (second form) (cddr form))))
    ;; An ordinary lambda list only: in a macro's, `()` is an empty
    ;; destructuring pattern, which the rewrite turns into a parameter named
    ;; nil — `(macrolet ((m (()) ...)) ...)` is legal (ansi-test macrolet.39).
    (%refuse-constants (third rewritten))
    (%compile-rewritten-lambda (first rewritten)
                               (second rewritten)
                               (third rewritten)
                               env)))

(defun %compile-rewritten-lambda (params body names env)
  (%emit (list 'CLOSURE (%constant-index (%assemble-lambda params body names env)))))

;;; Compile a function body on its own and hand back the verified function,
;;; saving and restoring the accumulators of whatever was being compiled
;;; around it. Split out of the above so that `macrolet` can have a function
;;; *now*, at compile time, rather than an instruction that makes one later.
(defun %assemble-lambda (params body names env)
  (let ((outer-code %code)
        (outer %constants)
        (outer-count %constant-count))
    (setq env (%notinline-scope body env))
    (setq %code nil)
    (%reset-constants)
    (%with-open ':function
                (lambda ()
                  (let ((declared (%declared-specials body)))
                    (if (%specials-among names declared)
                        ;; A parameter that names a special variable binds it
                        ;; dynamically for the call — `(defun f (*x*) ...)`.
                        (let ((renamed (%rename-specials names declared)))
                          (%compile-with-dynamic-binds
                           names renamed body
                           (%special-scope declared (cons (%scope ':variable renamed) env))
                           declared))
                        (%compile-body body
                                       (%special-scope declared
                                                       (cons (%scope ':variable names) env))
                                       t)))))
    (%emit (list 'RET))
    (let ((compiled (%assemble (reverse %code) (%constants-vector) params 'lambda)))
      (setq %code outer-code)
      (setq %constants outer)
      (setq %constant-count outer-count)
      compiled)))

(defun %compile-arguments (arguments env)
  (if (endp arguments)
      nil
      (progn (%compile (car arguments) env nil)
             (%compile-arguments (cdr arguments) env))))

(defun %compile-callee (head env)
  (cond ((symbolp head)
         ;; A local function first, which is what makes this a Lisp-2 worth
         ;; having: `(flet ((list (x) x)) (list 1))` calls the local one and
         ;; the variable `list` is untouched.
         (let ((place (%lookup head env 0 ':function)))
           (if place
               (%emit (list 'FREF (car place) (cdr place)))
               (%emit (list 'GFREF (%constant-index head))))))
        ;; `((lambda (x) ...) 1)` — a lambda expression in operator position,
        ;; which is a call like any other once the closure is on the stack.
        ((and (consp head) (eq (car head) 'lambda)) (%compile-lambda head env))
        (t (error "a call wants a symbol or a lambda expression as its operator"))))

;;; `(f a b)` — the callee first, then the arguments, then CALL.
;;;
;;; GFREF reads the symbol's function cell at run time, so redefining `f`
;;; takes effect. That is right for a user's function and wasteful for `+`,
;;; which cannot be redefined without undefined consequences (CLHS 11.1.2.1.2)
;;; and could compile to a PRIM instruction instead. Not yet.
;;;
;;; A call in tail position reuses the current frame instead of pushing one,
;;; so a loop written as recursion runs in constant space. Unlike the
;;; interpreter, which gets this by popping a frame before dispatching and so
;;; *discovers* that a call was in tail position, the compiler **decides** it:
;;; tail position is a property of the source, written into the output where a
;;; disassembler can show it.
;;;
;;; What must *cancel* tail position: `block`, `catch`, `unwind-protect` and
;;; `handler-case` — every form that leaves a record on the stack. Each leaves a record on the
;;; stack that has to outlive the call, and `TCALL` reuses the frame by
;;; dropping everything above it. The verifier refuses a `TCALL` with a record
;;; still open, so getting this wrong is a build failure and not a lost
;;; cleanup.
(defun %compile-call (form env tail)
  (%compile-callee (car form) env)
  (%compile-arguments (cdr form) env)
  (%emit (list (if tail 'TCALL 'CALL) (length (cdr form)))))

;;; `(flet ((f (x) ...)) ...)`
;;;
;;; The closures are made *before* the scope exists, in the enclosing
;;; environment — which is exactly what distinguishes `flet` from `labels`:
;;; an `flet` function cannot see itself or its siblings.
(defun %compile-local-functions (definitions env)
  (if (endp definitions)
      nil
      (progn (%compile-lambda (%local-lambda (car definitions)) env)
             (%compile-local-functions (cdr definitions) env))))

;;; A local function as a lambda expression. Its body is in a block named
;;; after it, as a `defun`'s is (CLHS 3.1.2.1.2.4); for `(setf f)`, named f.
;;; Like `defun`, only when the body returns from it, so a local function
;;; that loops by calling itself keeps its tail calls (#78).
(defun %local-lambda (definition)
  (let ((name (car definition)))
    (cons 'lambda
          (cons (second definition)
                (%block-body (if (consp name) (second name) name) (cddr definition))))))

;;; The names a `flet` or `labels` binds, each as the symbol the scope holds
;;; it under: a setf function's, `(setf f)`, is the symbol
;;; `%setf-function-symbol` keeps for f, which `#'(setf f)` also finds.
(defun %local-function-names (definitions)
  (if (endp definitions)
      nil
      (cons (%function-name-symbol (car (car definitions)))
            (%local-function-names (cdr definitions)))))

(defun %compile-flet (form env tail)
  (let ((names (%local-function-names (second form))))
    (%compile-local-functions (second form) env)
    (%emit (list 'BINDF (%constant-index (apply #'vector names))))
    (%compile-local-function-body (cddr form) names (cons (%scope ':function names) env) tail)
    (%emit (list 'UNBIND 1))))

;;; The body of a `flet` or `labels`: its declarations apply as `locally`'s
;;; do, and a local function hides a `macrolet` macro of the same name from
;;; further out, as it hides a global one (CLHS 3.1.1.2; ansi-test
;;; macrolet.37, #80). `%local-macros` gets an entry with no expander for
;;; each name, which `%find-macro` finds first and answers nil for.
(defun %compile-local-function-body (body names env tail)
  (let ((outer %local-macros))
    (setq %local-macros (%shadowing-entries names outer))
    (%compile-body body (%special-scope (%declared-specials body) env) tail)
    (setq %local-macros outer)))

(defun %shadowing-entries (names macros)
  (if (endp names)
      macros
      (cons (cons (car names) nil) (%shadowing-entries (cdr names) macros))))

;;; `(labels ((f (x) ... (f ...))) ...)`
;;;
;;; The functions can call themselves and each other, so the closures have to
;;; capture a scope that already exists — which means the scope is made first,
;;; with nothing in it, and the closures are stored into it afterwards.
;;;
;;; **That is the instruction the specification did not have.** There was
;;; `LSET` for a variable and `GSET` for a global and nothing that writes the
;;; function namespace, because `flet` does not need one and writing the VM
;;; never reached a case that did. `FSET` is the thirty-seventh opcode, and it
;;; exists because the compiler asked for it.
(defun %emit-nils (n)
  (if (= n 0)
      nil
      (progn (%emit (list 'NIL)) (%emit-nils (- n 1)))))

(defun %store-local-functions (definitions env index)
  (if (endp definitions)
      nil
      (progn (%compile-lambda (%local-lambda (car definitions)) env)
             (%emit (list 'FSET 0 index))
             (%emit (list 'POP))
             (%store-local-functions (cdr definitions) env (+ index 1)))))

(defun %compile-labels (form env tail)
  (let ((names (%local-function-names (second form))))
    (let ((inner (cons (%scope ':function names) env)))
      (%emit-nils (length names))
      (%emit (list 'BINDF (%constant-index (apply #'vector names))))
      (%store-local-functions (second form) inner 0)
      (%compile-local-function-body (cddr form) names inner tail)
      (%emit (list 'UNBIND 1)))))

;;; `(block out ...)` and `(return-from out value)`
;;;
;;; `BLOCK` makes a fresh marker, binds it in the environment under the
;;; block's name, and pushes a record saying where to carry on. `RETFROM`
;;; reads the marker back out — by `(depth, index)`, like any other variable,
;;; except in the block namespace — and unwinds towards it.
;;;
;;; Through the environment rather than a known stack depth, because a
;;; `return-from` can be inside a closure that has escaped, and then nobody
;;; knows how deep the stack is. Chapter 27 has the argument.
;;;
;;; **Tail position is cancelled inside a block.** The record has to outlive
;;; anything the body calls, and `TCALL` reuses the frame by dropping
;;; everything above it — so a tail call here would drop the block. The
;;; verifier refuses a `TCALL` with a record still open, so forgetting this
;;; would be a build failure rather than a vanished block; passing `nil` is
;;; what makes it never arise.
(defun %compile-block (form env)
  (let ((done (gensym))
        (name (second form)))
    (%emit (list 'BLOCK (%constant-index name) done))
    (%with-open ':record
                (lambda () (%compile-body (cddr form) (cons (%scope ':block (list name)) env) nil)))
    (%emit (list 'UNMARK))
    (%emit-label done)))

(defun %compile-return-from (form env)
  (let ((place (%lookup (second form) env 0 ':block)))
    (if place
        (progn (%compile (third form) env nil)
               (%emit (list 'RETFROM (car place) (cdr place))))
        (error "return-from names a block that is not here"))))

;;; Forms evaluated for their effect: each compiled and its value popped.
(defun %compile-for-effect (forms env)
  (if (endp forms)
      nil
      (progn (%compile (car forms) env nil)
             (%emit (list 'POP))
             (%compile-for-effect (cdr forms) env))))

;;; `(catch tag body...)` and `(throw tag value)`
;;;
;;; `block` with the identity found at run time instead of in the source:
;;; `CATCH` pops the tag and pushes a record holding it, and a `THROW`
;;; anywhere below — in this function or in one it calls, which a
;;; `return-from` could never reach — unwinds to the nearest record whose tag
;;; is the same object. The body is not in tail position, for `block`'s
;;; reason: the record has to outlive what it calls.
(defun %compile-catch (form env)
  (let ((done (gensym)))
    (%compile (second form) env nil)
    (%emit (list 'CATCH done))
    (%with-open ':record (lambda () (%compile-body (cddr form) env nil)))
    (%emit (list 'UNMARK))
    (%emit-label done)))

(defun %compile-throw (form env)
  (%compile (second form) env nil)
  (%compile (third form) env nil)
  (%emit (list 'THROW)))

;;; `(unwind-protect protected cleanup...)`
;;;
;;; The cleanup must run however the protected form is left, and there are
;;; two ways out, so **the cleanup is compiled twice** — `docs/bytecode.md`
;;; chose that over a way of sharing one copy, which would need a return
;;; address and so a frame:
;;;
;;;     PROTECT  ->cleanup
;;;     <protected>
;;;     UNMARK                 left normally: drop the record...
;;;     MVSAVE                 ...keep every value it produced...
;;;     <cleanup, for effect>  ...run the cleanup...
;;;     MVRESTORE              ...and give the values back
;;;     JMP      ->done
;;;   cleanup:                 left by an unwind passing through
;;;     <cleanup, for effect>
;;;     UNPROTECT              carry on unwinding
;;;   done:
;;;
;;; `MVSAVE` and `MVRESTORE` are there because `unwind-protect` returns every
;;; value of its protected form, and the cleanup in between would otherwise
;;; overwrite them (chapter 23).
(defun %compile-unwind-protect (form env)
  (let ((cleanup (gensym))
        (done (gensym)))
    (%emit (list 'PROTECT cleanup))
    (%with-open ':record (lambda () (%compile (second form) env nil)))
    (%emit (list 'UNMARK))
    (%emit (list 'MVSAVE))
    (%compile-for-effect (cddr form) env)
    (%emit (list 'MVRESTORE))
    (%emit (list 'JMP done))
    (%emit-label cleanup)
    ;; This copy runs with a paused record on the stack; the inline one above
    ;; runs after `UNMARK`, with none.
    (%with-open ':record (lambda () (%compile-for-effect (cddr form) env)))
    (%emit (list 'UNPROTECT))
    (%emit-label done)))

;;; `(multiple-value-prog1 first rest...)`: every value of FIRST, after the
;;; rest have run for effect. With no rest it is just FIRST, and may be in
;;; tail position.
(defun %compile-multiple-value-prog1 (form env tail)
  (if (endp (cddr form))
      (%compile (second form) env tail)
      (progn (%compile (second form) env nil)
             (%emit (list 'MVSAVE))
             (%compile-for-effect (cddr form) env)
             (%emit (list 'MVRESTORE)))))

;;; `(defmacro name lambda-list body...)`
;;;
;;; Until #33 this was refused: while the compiler is interpreted, a compiled
;;; expander is one it could never call, so the bootstrap rule (#25) had the
;;; interpreter evaluate every `defmacro`. Building a compiled image ends
;;; that, because the compiler that will call these expanders is compiled
;;; too.
;;;
;;; It compiles to what the interpreter's `defmacro` does by hand:
;;;
;;;     (progn (%set-symbol-function 'name (%make-macro <the expander>))
;;;            'name)
;;;
;;; The expander is an ordinary compiled closure over the macro's argument
;;; forms, its parameters rewritten in *macro* mode — destructuring, `&body`,
;;; a dotted rest — by the same Rust the interpreter uses, so the two take a
;;; call apart identically. `%make-macro` gives it the macro type, which is
;;; what `macroexpand-1` looks for.
;;;
;;; The body is in a block named after the macro when it returns from one,
;;; as a function's is (`%block-body`, and CLHS `defmacro`); so is a
;;; `macrolet` expander's below. Both were missed when `defun`, `flet` and
;;; `labels` got theirs (#109).
(defun %compile-defmacro (form env)
  (let ((name (second form))
        (rewritten (%rewrite-macro-lambda-list
                    (third form)
                    (%block-body (second form) (cdr (cddr form))))))
    (%compile-callee '%set-symbol-function env)
    (%compile-value name)
    (%compile-callee '%make-macro env)
    (%compile-rewritten-lambda (first rewritten) (second rewritten) (third rewritten) env)
    (%emit (list 'CALL 1))
    (%emit (list 'CALL 2))
    (%emit (list 'POP))
    (%compile-value name)))

;;; `(macrolet ((name lambda-list body...) ...) body...)`
;;;
;;; Local macros, which exist only while the body is being *compiled*: the
;;; expanders run in the compiler and nothing of them reaches the output.
;;; Each is compiled here and now into a function, made a macro, and held in
;;; `%local-macros` for the body's extent, where `%compile-cons` consults it
;;; before any global macro.
;;;
;;; Only a compiled compiler can do this, because the expanders it makes are
;;; compiled and it has to call them. Run by the interpreted compiler, the
;;; first use is refused by the one-evaluator rule — which is the right
;;; answer, and the reason this waited for #33.
(setq %local-macros nil)

;;; Its body's declarations apply as `locally`'s do (ansi-test macrolet.47).
(defun %compile-macrolet (form env tail)
  (let ((outer %local-macros))
    (setq %local-macros (%local-expanders (second form) outer))
    (%compile-body (cddr form) (%special-scope (%declared-specials (cddr form)) env) tail)
    (setq %local-macros outer)))

(defun %local-expanders (definitions macros)
  (if (endp definitions)
      macros
      (%local-expanders (cdr definitions)
                        (cons (cons (car (car definitions))
                                    (%local-expander (car definitions)))
                              macros))))

(defun %local-expander (definition)
  (let ((rewritten (%rewrite-macro-lambda-list
                    (second definition)
                    (%block-body (car definition) (cddr definition)))))
    (%make-macro (%assemble-lambda (first rewritten) (second rewritten) (third rewritten) nil))))

;;; The local macro named HEAD, innermost first, or nil.
(defun %local-macro (head)
  (%find-macro head %local-macros))

(defun %find-macro (head macros)
  (cond ((endp macros) nil)
        ((eq (car (car macros)) head) (cdr (car macros)))
        (t (%find-macro head (cdr macros)))))

;;; `(symbol-macrolet ((name expansion) ...) body...)` (#53)
;;;
;;; A symbol that stands for a form. Like `macrolet`, it exists only while
;;; the body is compiled, but it lives in ENV rather than beside it, because
;;; it is a *variable*-like name and must obey variable scope: a `let` of the
;;; same name inside the body shadows it, and that falls out of keeping both
;;; in the one chain, innermost first. `%lookup` steps over the scope without
;;; counting it, since nothing exists for it at run time.
(defun %compile-symbol-macrolet (form env tail)
  (let ((definitions (%symbol-macro-definitions (second form)))
        (declared (%declared-specials (cddr form))))
    ;; A body may declare names special, as `locally`'s may; declaring one of
    ;; its own symbol macros special is a `program-error` (CLHS). Found when
    ;; ansi-test could first load this file (#68).
    (if (%symbol-macro-declared-special-p definitions declared)
        (error 'program-error))
    (%compile-body (cddr form)
                   (%special-scope declared (cons (cons ':symbol-macro definitions) env))
                   tail)))

(defun %symbol-macro-declared-special-p (definitions declared)
  (cond ((endp definitions) nil)
        ((member (car (car definitions)) declared) t)
        (t (%symbol-macro-declared-special-p (cdr definitions) declared))))

;;; `((name expansion) ...)` as an alist. A special variable cannot be one
;;; (CLHS `symbol-macrolet`): its references go to the symbol at run time.
(defun %symbol-macro-definitions (definitions)
  (if (endp definitions)
      nil
      (let ((name (car (car definitions))))
        (if (%special-variable-p name)
            (error "symbol-macrolet cannot name a special variable" name))
        (%refuse-constants (list name))
        (cons (cons name (second (car definitions)))
              (%symbol-macro-definitions (cdr definitions))))))

;;; What NAME stands for in ENV, as `(name . expansion)`, or nil: nil also
;;; when a variable binding nearer than the definition shadows it. Past the
;;; outermost scope, a global symbol macro (#68), which any lexical binding
;;; of the name therefore hides.
(defun %symbol-macro (name env)
  (cond ((null env) (%global-symbol-macro name))
        ((eq (car (car env)) ':symbol-macro)
         (or (%assq name (cdr (car env))) (%symbol-macro name (cdr env))))
        ((and (eq (car (car env)) ':variable) (%position name (cdr (car env)) 0)) nil)
        ((and (eq (car (car env)) ':special) (member name (cdr (car env)))) nil)
        (t (%symbol-macro name (cdr env)))))

;;; Global symbol macros, `define-symbol-macro`'s (#68, decision 3): name to
;;; `(name . expansion)`. The table is the compiler's, as `%compiler-macros`
;;; is, so that it can be read while the compiler runs interpreted; it is
;;; empty whenever the compiler compiles itself, which keeps the fixpoint. A
;;; library symbol macro naming a variable the compiler reads would break it.
(setq %global-symbol-macros (make-hash-table))

(defun %global-symbol-macro (name)
  (if (symbolp name) (values (gethash name %global-symbol-macros)) nil))

(defun %assq (key alist)
  (cond ((endp alist) nil)
        ((eq (car (car alist)) key) (car alist))
        (t (%assq key (cdr alist)))))

;;; The environment a macro sees (#53)
;;;
;;; A macro is a function of its form, and some need more: to know
;;; what the code *around* the call means. `(incf x)` must ask whether `x`
;;; is a symbol macro here, and only the compiler knows that. Common Lisp
;;; hands an expander that asks — with `&environment` in its lambda list —
;;; the **lexical environment** of the call as an object, which
;;; `macroexpand-1` and `get-setf-expansion` accept.
;;;
;;; Here the object is `(:environment scopes local-macros)`: the compiler's
;;; ENV, and `%local-macros`, as they stand while the expander runs. The
;;; compiler records ENV in `%expansion-scopes` for the length of each
;;; expansion; nil for both is the null environment, and is written nil.
(setq %expansion-scopes nil)

;;; Call EXPANDER on FORM with SCOPES and MACROS as its environment. An
;;; expander is a function of the form and the environment, a macro's and a
;;; compiler macro's alike (#76). SCOPES and MACROS stay recorded while it
;;; runs, as they were when an expander asked for them (#53).
(defun %expand (expander form scopes macros)
  (let ((outer-scopes %expansion-scopes)
        (outer-macros %local-macros))
    (setq %expansion-scopes scopes)
    (setq %local-macros macros)
    (let ((expansion (funcall expander form (%current-environment))))
      (setq %expansion-scopes outer-scopes)
      (setq %local-macros outer-macros)
      expansion)))

(defun %current-environment ()
  (if (or %expansion-scopes %local-macros)
      (list ':environment %expansion-scopes %local-macros)
      nil))

;;; ------------------------------------------------------------------------
;;; Compiler macros (#65)
;;;
;;; A compiler macro is an optional rewrite of calls to a global function: a
;;; function of the call form and the environment, which returns a form to
;;; compile instead, or the call itself to decline. CLHS 3.2.2.1.3 lets a
;;; compiler ignore them; this one applies them (decision 1). The table is
;;; here, in the compiler, because the compiler must be able to read it even
;;; when it runs interpreted in the cold build — where it is always empty, and
;;; so is in a rebuild until a program defines one. That emptiness is what
;;; keeps the fixpoint: no compiler macro applies to the compiler's own calls.
(setq %compiler-macros (make-hash-table))

;;; The compiler macro for this call, if one applies: the call is to a global
;;; function — named in operator position or as `(funcall #'name ...)` — that
;;; no local function shadows, and that has one.
(defun %compiler-macro-for (form env)
  (let ((head (car form)))
    (cond ((and (eq head 'funcall) (consp (cdr form)) (consp (second form))
                (eq (car (second form)) 'function) (consp (cdr (second form)))
                (symbolp (second (second form))))
           (%global-compiler-macro (second (second form)) env))
          ((symbolp head) (%global-compiler-macro head env))
          (t nil))))

(defun %global-compiler-macro (name env)
  (if (%lookup name env 0 ':function) nil (gethash name %compiler-macros)))

;;; FORM rewritten by its compiler macro, or FORM itself.
(defun %apply-compiler-macro (form env)
  (let ((expander (if (%notinline-p (car form) env) nil (%compiler-macro-for form env))))
    (if expander
        (%expand expander form env %local-macros)
        form)))

(defun %environment-symbol-macro (environment name)
  (if environment (%symbol-macro name (second environment)) (%global-symbol-macro name)))

(defun %environment-macro (environment name)
  (if environment (%find-macro name (third environment)) nil))

;;; Whether NAME is a local function in ENVIRONMENT, which then shadows a
;;; global macro of that name.
(defun %environment-function-p (environment name)
  (if environment (%lookup name (second environment) 0 ':function) nil))

;;; `(%destructuring-lambda lambda-list body...)` (#54): a function whose
;;; parameters are a *macro* lambda list — destructuring, `&body`, a dotted
;;; rest — for `destructuring-bind` in the library. The rewrite is the one
;;; `defmacro` uses, so the two take a list apart identically; its output
;;; is compiled directly, because rewriting it a second time as an ordinary
;;; `lambda` would lose the `&optional` defaults.
(defun %compile-destructuring-lambda (form env)
  (let ((rewritten (%rewrite-macro-lambda-list (second form) (cddr form) t)))
    (%compile-rewritten-lambda (first rewritten) (second rewritten) (third rewritten) env)))

;;; `(%macro-lambda lambda-list body...)` (#80): the expander `defmacro`
;;; makes, a function of the form and the environment, on its own — what
;;; the library's `defmacro` macro expands into, so that `defmacro` can be a
;;; macro as CL says while the compiler still compiles a `defmacro` form
;;; directly, as the interpreter evaluates one.
(defun %compile-macro-lambda (form env)
  (let ((rewritten (%rewrite-macro-lambda-list (second form) (cddr form))))
    (%compile-rewritten-lambda (first rewritten) (second rewritten) (third rewritten) env)))

;;; `(%dynamic-bind symbol value body...)` (#53): bind the special variable
;;; that SYMBOL — a form, evaluated — names, to VALUE, for BODY. Not Common
;;; Lisp: `progv` in the library is built on it, one name at a time.
(defun %compile-dynamic-bind (form env)
  (%compile (second form) env nil)
  (%compile (third form) env nil)
  (%emit (list 'DBIND-SYMBOL))
  (%with-open ':record (lambda () (%compile-body (cdr (cddr form)) env nil)))
  (%emit (list 'UNMARK)))

;;; `(tagbody tag-or-form...)` and `(go tag)`
;;;
;;; Every loop in Lisp is one of these underneath, so whatever this compiles
;;; to, every loop pays for. #37 has the argument; the shape is:
;;;
;;;     TAG    #(tag...) ->table   bind every tag to one marker, push a record
;;;   tag0:
;;;     <statement> POP ...
;;;   tag1:
;;;     <statement> POP ...
;;;     NIL                        fell off the end: a tagbody yields nil
;;;     UNMARK
;;;     UNBIND 1
;;;     JMP    ->done
;;;   table:                       a go lands here, plus 3 x its tag's index
;;;     JMP    ->tag0
;;;     JMP    ->tag1
;;;   done:
;;;
;;; `(go tag)` is `GO depth index`: found through the environment like any
;;; variable, so it works from inside a closure, and carried out by the
;;; unwinder, so the stack, the scopes and any `unwind-protect` cleanups in
;;; between are all dealt with. The record survives the landing, so the
;;; tagbody can be gone to again.
;;;
;;; Which items are tags is the interpreter's rule: **anything that is not a
;;; list**. A tagbody with no tags at all is just its statements and a `nil`.
(defun %tagbody-tags (items)
  (cond ((endp items) nil)
        ((consp (car items)) (%tagbody-tags (cdr items)))
        (t (cons (car items) (%tagbody-tags (cdr items))))))

(defun %labels-for (tags)
  (if (endp tags)
      nil
      (cons (gensym) (%labels-for (cdr tags)))))

(defun %compile-tagbody-items (items tags labels env)
  (cond ((endp items) nil)
        ((consp (car items))
         (%compile (car items) env nil)
         (%emit (list 'POP))
         (%compile-tagbody-items (cdr items) tags labels env))
        (t (%emit-label (nth (%position (car items) tags 0) labels))
           (%compile-tagbody-items (cdr items) tags labels env))))

(defun %emit-jumps (labels)
  (if (endp labels)
      nil
      (progn (%emit (list 'JMP (car labels)))
             (%emit-jumps (cdr labels)))))

(defun %compile-tagbody (form env)
  (let ((tags (%tagbody-tags (cdr form))))
    (if (endp tags)
        (progn (%compile-for-effect (cdr form) env)
               (%emit (list 'NIL)))
        (let ((labels (%labels-for tags))
              (table (gensym))
              (done (gensym))
              (scope (%scope ':tag tags)))
          (%emit (list 'TAG (%constant-index (apply #'vector tags)) table))
          (%with-open (list ':tagbody scope tags labels)
                      (lambda ()
                        (%compile-tagbody-items (cdr form) tags labels (cons scope env))))
          (%emit (list 'NIL))
          (%emit (list 'UNMARK))
          (%emit (list 'UNBIND 1))
          (%emit (list 'JMP done))
          (%emit-label table)
          (%emit-jumps labels)
          (%emit-label done)))))

;;; `(go tag)`: one step if it can be, an unwind if it must be.
;;;
;;; `JMP-TAG` needs the tagbody's record to be the innermost one, so it is
;;; used only when the tagbody is the innermost thing open — no nested
;;; function between (a closure may run anywhere, and only the unwinder can
;;; get back from there), no other record (a `block` or `catch` to drop, an
;;; `unwind-protect` cleanup to run), and no inner tagbody. Everything else
;;; between — values on the stack, `let` scopes — the record undoes by
;;; itself. Otherwise it is `GO`, found through the environment and carried
;;; out by the unwinder, which is always right and three steps instead of one.
(defun %compile-go (form env)
  (let ((place (%lookup (second form) env 0 ':tag)))
    (if (null place)
        (error "go names a tag that is not here")
        (let ((entry (%innermost-tagbody (%tag-scope (second form) env))))
          (if entry
              (%emit (list 'JMP-TAG
                           (nth (%position (second form) (third entry) 0)
                                (nth 3 entry))))
              (%emit (list 'GO (car place) (cdr place))))))))

;;; The scope a tag lives in: the innermost `:tag` scope that names it, which
;;; is the one `%lookup` found.
(defun %tag-scope (tag env)
  (cond ((null env) nil)
        ((and (eq (car (car env)) ':tag) (%position tag (cdr (car env)) 0)) (car env))
        (t (%tag-scope tag (cdr env)))))

;;; The open entry for the tagbody whose scope is SCOPE — but only if it is
;;; the innermost thing open, which is exactly when `JMP-TAG` is safe.
(defun %innermost-tagbody (scope)
  (let ((innermost (car %open)))
    (if (and (consp innermost) (eq (second innermost) scope))
        innermost
        nil)))

;;; `(eval-when (situations...) body...)`: the body if the situations include
;;; `execute`, otherwise `nil`. The interpreter's rule exactly, including the
;;; three spellings it accepts until there are packages — two evaluators that
;;; disagree is what #36 cost. Decided here, at compile time, so the output
;;; contains either the body or a `NIL` and never a test.
(defun %executes-p (situations)
  (cond ((endp situations) nil)
        ((member (car situations) '(execute :execute eval)) t)
        (t (%executes-p (cdr situations)))))

;;; `(handler-case form (type (var) body...) ...)`
;;;
;;; The prelude's macro turns that into `(%handler-case form clause...)`, and
;;; until #36 nothing here compiled it — it became a call to a function named
;;; `%HANDLER-CASE`, which does not exist.
;;;
;;; The shape is `docs/bytecode.md`'s. `HANDLER` pushes a record that stops
;;; *every* condition; the VM knows that a handler is here and nothing about
;;; which conditions it wants. Deciding that is ordinary compiled code at the
;;; HANDLER label, where the unwinder leaves the condition on the stack:
;;;
;;;     HANDLER clauses  ->handler
;;;     <form>
;;;     UNMARK
;;;     JMP              ->done
;;;   handler:
;;;     BIND #(c)
;;;     <(cond ((matches c type) (let ((var c)) body...)) ... (t (error c)))>
;;;     UNBIND 1
;;;   done:
;;;
;;; Which keeps the judgement above the primitive table (rule 1), and costs a
;;; `cond` in the output that a VM-side matcher would not. The last clause
;;; re-signals a condition nobody here wanted; the record has already been
;;; popped by then, so the search carries on further out.
;;;
;;; **Tail position.** The protected form is not in it: the record has to
;;; outlive anything it calls. The clause bodies are, because by the time
;;; they run the record is gone.
(defun %compile-handler-case (form env tail)
  (let ((handler (gensym))
        (done (gensym))
        (condition (gensym)))
    (%emit (list 'HANDLER (%constant-index (cddr form)) handler))
    (%with-open ':record (lambda () (%compile (second form) env nil)))
    (%emit (list 'UNMARK))
    (%emit (list 'JMP done))
    (%emit-label handler)
    (%emit (list 'BIND (%constant-index (vector condition))))
    (%compile (cons 'cond (%handler-clauses condition (cddr form)))
              (cons (%scope ':variable (list condition)) env)
              tail)
    (%emit (list 'UNBIND 1))
    (%emit-label done)))

;;; One `cond` clause per `handler-case` clause, and a last one that
;;; re-signals.
;;;
;;; Except after a catch-all. A clause for `error` takes everything, so
;;; nothing written after it can run — including the re-signal, which the
;;; first version emitted anyway and a test counting tail calls found. The
;;; verifier would have accepted the dead code; a reader of the disassembly
;;; would have wondered what it was for.
(defun %handler-clauses (condition clauses)
  (cond ((endp clauses) (list (list t (list 'error condition))))
        ((eq (%handler-test condition (car (car clauses))) t)
         (list (%handler-clause condition (car clauses))))
        (t (cons (%handler-clause condition (car clauses))
                 (%handler-clauses condition (cdr clauses))))))

;;; `(type (var) body...)` or `(type () body...)`.
(defun %handler-clause (condition clause)
  (let ((variables (second clause))
        (body (cddr clause)))
    (list (%handler-test condition (car clause))
          (if variables
              (cons 'let (cons (list (list (car variables) condition)) body))
              (cons 'progn body)))))

;;; Does a clause for TYPE want CONDITION?
;;;
;;; The same flat matching as `condition::matches` in Rust — `t`, `error` and
;;; `condition` take everything, anything else must name the condition's own
;;; type — but decided at **compile time** where it can be: the clause's type
;;; is in the source, so a catch-all clause compiles to no test at all.
;;;
;;; A clause may name several types as `(or a b)`, which is the one compound
;;; type specifier worth having before `typep` exists: the tests of each. Any
;;; other compound specifier is refused here, when the clause is compiled.
;;; It used to compile into a test that was never true, so the clause never
;;; ran and the error it was written for went past it (#106).
(defun %handler-test (condition type)
  (cond ((or (eq type t) (eq type 'error) (eq type 'condition)) t)
        ;; A subtype is caught too: `stream-error` catches `end-of-file` (#81).
        ;; The subtypes are listed now, from the prelude's table, so that the
        ;; test is still `eq` on the type, which compiled code can do before
        ;; the prelude is compiled (the bootstrap's own tests run it so).
        ((symbolp type)
         (cons 'or (%condition-type-tests condition (%condition-subtypes type))))
        ((and (consp type) (eq (car type) 'or))
         (%handler-any-test condition (cdr type)))
        (t (%malformed "handler-case"
                       "a clause's type is a symbol or (or type...); other compound types need typep, which is phase 3"))))

(defun %handler-any-test (condition types)
  (if (endp types)
      nil
      (let ((first (%handler-test condition (car types)))
            (rest (%handler-any-test condition (cdr types))))
        (cond ((eq first t) t)
              ((eq rest t) t)
              (t (list 'or first rest))))))

(defun %condition-type-tests (condition types)
  (if (endp types)
      nil
      (cons (list 'eq (list '%condition-type condition) (list 'quote (car types)))
            (%condition-type-tests condition (cdr types)))))

;;; `(multiple-value-call f a b)`
;;;
;;; Each argument form is compiled and then `MVSAVE` replaces its value with
;;; the list of *every* value it produced — so `(values 1 2)` contributes two
;;; arguments and not one. `MVCALL` appends the lists and calls.
;;;
;;; The function is an ordinary form, not a name: `(multiple-value-call #'list
;;; ...)` evaluates `#'list` like anything else.
(defun %compile-mv-arguments (arguments env)
  (if (endp arguments)
      nil
      (progn (%compile (car arguments) env nil)
             (%emit (list 'MVSAVE))
             (%compile-mv-arguments (cdr arguments) env))))

(defun %compile-multiple-value-call (form env)
  (%compile (second form) env nil)
  (%compile-mv-arguments (cddr form) env)
  (%emit (list 'MVCALL (length (cddr form)))))

;;; The interpreter's syntax rules for the special operators, checked before
;;; anything is compiled.
;;;
;;; Until step E (#38) the compiler checked almost nothing: `(let)` compiled
;;; as a `let` with no bindings, `(block)` as a block named `nil`, `(flet ((f))
;;; 1)` as a function with no parameters. The interpreter refuses all of them,
;;; and so does SBCL, and the differential files say so — but no file had been
;;; run compiled. Each condition below is the interpreter's, read from
;;; `eval.rs` rather than guessed from its message, because two evaluators that
;;; disagree about what is a program are two languages.
(defun %malformed (operator detail)
  (error (%string-append "bad " (%string-append operator (%string-append ": " detail)))))

;;; A binding list: nil, or a list whose elements are each a symbol or a list
;;; starting with one.
(defun %bindings-p (bindings)
  (cond ((not (consp bindings)) t)
        ((symbolp (car bindings)) (%bindings-p (cdr bindings)))
        ((and (consp (car bindings)) (symbolp (car (car bindings))))
         (%bindings-p (cdr bindings)))
        (t nil)))

;;; Local function definitions: each `(name (params) body...)`, where a
;;; `flet` or `labels` NAME may also be `(setf symbol)` (SETF-NAMES-P).
(defun %function-bindings-p (bindings setf-names-p)
  (cond ((not (consp bindings)) t)
        ((and (consp (car bindings))
              (consp (cdr (car bindings)))
              (or (symbolp (car (car bindings)))
                  (and setf-names-p (%setf-function-name-p (car (car bindings))))))
         (%function-bindings-p (cdr bindings) setf-names-p))
        (t nil)))

;;; `setq`'s arguments: symbol, value, symbol, value...
(defun %setq-pairs-p (pairs)
  (cond ((null pairs) t)
        ((and (symbolp (car pairs)) (consp (cdr pairs))) (%setq-pairs-p (cddr pairs)))
        (t nil)))

(defun %check-syntax (head form)
  (let ((rest (cdr form)))
    (cond ((eq head 'quote)
           (if (not (and (consp rest) (null (cdr rest))))
               (%malformed "quote" "wants exactly one argument")))
          ((eq head 'if)
           (if (not (and (consp rest) (consp (cdr rest))))
               (%malformed "if" "wants a test and a consequent")))
          ((eq head 'setq)
           (if (not (%setq-pairs-p rest))
               (%malformed "setq" "wants alternating symbols and values")))
          ((or (eq head 'let) (eq head 'let*))
           (if (not (and (consp rest)
                         (or (null (car rest)) (consp (car rest)))
                         (%bindings-p (car rest))))
               (%malformed (if (eq head 'let) "let" "let*")
                           "wants a list of bindings, each (name init), (name) or name")))
          ((or (eq head 'flet) (eq head 'labels) (eq head 'macrolet))
           (if (not (and (consp rest)
                         (or (null (car rest)) (consp (car rest)))
                         (%function-bindings-p (car rest) (not (eq head 'macrolet)))))
               (%malformed "flet" "wants a list of (name (params) body...)")))
          ((eq head 'function)
           (if (not (and (consp rest)
                         (null (cdr rest))
                         (or (symbolp (car rest))
                             (%setf-function-name-p (car rest))
                             (and (consp (car rest)) (eq (car (car rest)) 'lambda)))))
               (%malformed "function" "wants exactly one name or lambda")))
          ((or (eq head 'block) (eq head 'return-from))
           (if (not (and (consp rest) (symbolp (car rest))))
               (%malformed (if (eq head 'block) "block" "return-from") "wants a name")))
          ((eq head 'defmacro)
           (if (not (and (consp rest) (consp (cdr rest)) (symbolp (car rest))))
               (%malformed "defmacro" "wants a name, a parameter list and a body")))
          ;; One required part, and nothing else to say about it.
          ((or (eq head 'lambda) (eq head 'catch) (eq head 'throw) (eq head 'go)
               (eq head 'unwind-protect) (eq head 'eval-when) (eq head 'the)
               (eq head 'multiple-value-call) (eq head 'multiple-value-prog1)
               (eq head '%handler-case))
           (if (not (consp rest))
               (%malformed (symbol-name head) "is missing a required part"))))))

;;; A list form: a special operator, a macro call, or a function call.
;;;
;;; The order matters and is the standard's. A special operator is not
;;; shadowable by a macro, and a macro call is expanded before anything looks
;;; at it as a call — so `when` never reaches `%compile-call`, and neither
;;; does anything a user defines with `defmacro`.
(defun %compile-cons (form env tail)
  (let ((head (car form)))
    (if (%special-operator-p head) (%check-syntax head form))
    (cond ((eq head 'quote) (%compile-value (second form)))
          ((eq head 'if) (%compile-if form env tail))
          ((eq head 'progn) (%compile-body (cdr form) env tail))
          ((eq head 'let) (%compile-let form env tail))
          ((eq head 'let*) (%compile-let* (second form) (cddr form) env tail))
          ((eq head 'setq) (%compile-setq form env))
          ((eq head 'lambda) (%compile-lambda form env))
          ;; `#'f` is `(function f)`, and what it needs is exactly what a
          ;; call's operator needs: a local function, a global one, or a
          ;; closure made on the spot.
          ((eq head 'function) (%compile-callee (%function-name-symbol (second form)) env))
          ((eq head 'block) (%compile-block form env))
          ((eq head '%handler-case) (%compile-handler-case form env tail))
          ((eq head 'tagbody) (%compile-tagbody form env))
          ((eq head 'go) (%compile-go form env))
          ((eq head 'defmacro) (%compile-defmacro form env))
          ((eq head 'macrolet) (%compile-macrolet form env tail))
          ((eq head 'symbol-macrolet) (%compile-symbol-macrolet form env tail))
          ((eq head '%dynamic-bind) (%compile-dynamic-bind form env))
          ((eq head '%destructuring-lambda) (%compile-destructuring-lambda form env))
          ((eq head '%macro-lambda) (%compile-macro-lambda form env))
          ((eq head 'catch) (%compile-catch form env))
          ((eq head 'throw) (%compile-throw form env))
          ((eq head 'unwind-protect) (%compile-unwind-protect form env))
          ((eq head 'multiple-value-prog1) (%compile-multiple-value-prog1 form env tail))
          ;; Declarations the interpreter does not check, so both reduce to
          ;; their body — the interpreter's rule, and `the` refuses the same
          ;; malformed form it does.
          ((eq head 'the)
           (if (consp (cdr form))
               (%compile-body (cddr form) env tail)
               (error "the wants a type and a form")))
          ((eq head 'locally)
           (%compile-body (cdr form)
                          (%notinline-scope (cdr form)
                                            (%special-scope (%declared-specials (cdr form)) env))
                          tail))
          ((eq head 'eval-when)
           (if (%executes-p (second form))
               (%compile-body (cddr form) env tail)
               (%emit (list 'NIL))))
          ((eq head 'return-from) (%compile-return-from form env))
          ((eq head 'multiple-value-call) (%compile-multiple-value-call form env))
          ((eq head 'flet) (%compile-flet form env tail))
          ((eq head 'labels) (%compile-labels form env tail))
          ;; A special operator with no case above is one this compiler cannot
          ;; compile, and saying so is the only honest answer. Until #37 it
          ;; fell through to the call below and became a call to a function
          ;; named after the operator, which compiled, verified, and failed
          ;; only when run — `handler-case` hid that way for a week.
          ;; A local macro shadows a global one, so it is asked first.
          ((and (symbolp head) (%local-macro head))
           (%compile (%expand (%local-macro head) form env %local-macros) env tail))
          ((%special-operator-p head)
           (error (%string-append "the compiler cannot compile the special operator "
                                  (symbol-name head))))
          ;; A local function shadows a global macro of the same name (CLHS `flet`):
          ;; `(flet ((when (x) x)) (when 1))` calls the
          ;; function. Until #54 the global macro won.
          ((and (symbolp head) (%lookup head env 0 ':function))
           (%compile-call form env tail))
          ;; A global macro, expanded here rather than by `macroexpand-1` so
          ;; the expander runs in this ENV (#53) — and so that the library
          ;; may redefine `macroexpand-1` without touching how the compiler
          ;; compiles itself.
          ;; Otherwise a call — unless a compiler macro rewrites it (#65).
          (t (let ((expander (if (symbolp head) (%macro-function head) nil)))
               (if expander
                   (%compile (%expand expander form env %local-macros) env tail)
                   (let ((rewritten (%apply-compiler-macro form env)))
                     (if (eq rewritten form)
                         (%compile-call form env tail)
                         (%compile rewritten env tail)))))))))

;;; One form, emitting instructions that leave exactly one value.
;;;
;;; TAIL says whether this form's value is the function's value. Only a call
;;; does anything with it; everything else leaves its value on the stack and
;;; the `RET` that follows does the returning.
;;;
;;; A symbol is a variable: found in the compile-time environment it is a pair
;;; of small numbers, and not found it is a global read through its symbol at
;;; run time. Which is chapter 26's distinction, and the reason for it — a
;;; lexical binding is created and destroyed by code the compiler can see in
;;; full, and a global is reachable by anything at any time.
(defun %compile (form env tail)
  (cond ((null form) (%emit (list 'NIL)))
        ((eq form t) (%emit (list 'T)))
        ((symbolp form)
         (let ((macro (%symbol-macro form env)))
           (if macro
               (%compile (cdr macro) env tail)
               (let ((place (%lookup form env 0 ':variable)))
                 (if place
                     (%emit (list 'LREF (car place) (cdr place)))
                     (%emit (list 'GREF (%constant-index form))))))))
        ((consp form) (%compile-cons form env tail))
        (t (%compile-value form))))

;;; Compile FORM into a function of no arguments.
;;;
;;; The entry point. The compiler keeps its work in five global variables, and
;;; one compilation can begin while another is half done: a macro's expander
;;; runs in the middle of compiling the form that uses it, and if the expander
;;; calls `eval`, that compiles too. Resetting the globals from nothing, as
;;; this once did, handed the outer compilation the inner one's code: no
;;; error, just a wrong answer (#47). So whatever was being compiled is saved
;;; first and given back afterwards — the save and restore `%assemble-lambda`
;;; already does for a lambda inside a form — under `unwind-protect`, so that
;;; a nested compilation that fails half-way gives it back as well.
;;;
;;; That makes compiling re-entrant, which is not the same as safe for two
;;; processes compiling at once: a switch between them would land in the
;;; middle of both. That is phase 3's question, and #34's.
(defun compile-form (form)
  (let ((outer-code %code)
        (outer-open %open)
        (outer-macros %local-macros)
        (outer-scopes %expansion-scopes)
        (outer-constants %constants)
        (outer-count %constant-count))
    (unwind-protect
         (progn
           (setq %code nil)
           (setq %open nil)
           (setq %local-macros nil)
           (setq %expansion-scopes nil)
           (%reset-constants)
           (%compile form nil t)
           (%emit (list 'RET))
           (%assemble (reverse %code) (%constants-vector) nil 'compiled-form))
      (setq %code outer-code)
      (setq %open outer-open)
      (setq %local-macros outer-macros)
      (setq %expansion-scopes outer-scopes)
      (setq %constants outer-constants)
      (setq %constant-count outer-count))))
