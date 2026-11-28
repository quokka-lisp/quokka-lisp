;;;; frames.lisp — the frames on the stack, read and retried (phase 3,
;;;; step D, #130; the design is `docs/frames.md`)
;;;;
;;;; Every call frame keeps the call that made it: the function called and
;;;; the arguments as they were passed, on the stack below the frame's
;;;; header (`vm::frame`). `%frames` reads them all, innermost first, and
;;;; this file makes them into `frame` objects a debugger can show and act
;;;; on: name a frame's call, print a backtrace, and unwind to a frame and
;;;; call something in its place — which is how a frame is retried, or made
;;;; to return a value of the debugger's choosing.
;;;;
;;;; The public names are in the package `QUOKKA-DEBUG` (`packages.lisp`), as
;;;; SBCL's are in `SB-DEBUG`, and mean what SBCL's of the same name mean:
;;;; `list-backtrace`, `print-backtrace`, `unwind-to-frame-and-call`.

;;; A frame as the debugger sees it: where it is on the stack, the token
;;; that names this one call to the machine (a later call at the same place
;;; has another), and the call it keeps. A snapshot: the frame it describes
;;; may since have returned, which `unwind-to-frame-and-call` checks before
;;; acting on it.
(defstruct (frame (:constructor %make-frame (place token function environment arguments))
                  (:copier nil))
  place token function environment arguments)

;;; `#<FRAME (FOO 1 2)>`.
(defmethod print-object ((frame frame) stream)
  (print-unreadable-object (frame stream :type t)
    (let ((*print-level* 3) (*print-length* 5))
      (prin1 (frame-call frame) stream))))

;;; The frames open below the caller, innermost first; at most COUNT of them.
;;; The frame of `frames` itself is left out, so the first is its caller's.
(defun frames (&optional count)
  (unless (or (null count) (and (integerp count) (>= count 0)))
    (error 'type-error :datum count :expected-type '(or null (integer 0))))
  (let ((found nil) (n 0))
    ;; One more than COUNT is asked of the machine: its first is this
    ;; function's own frame.
    (dolist (entry (cdr (%frames (if count (+ count 1) nil))) (nreverse found))
      (when (and count (>= n count)) (return (nreverse found)))
      (push (%make-frame (first entry) (second entry) (third entry) (fourth entry)
                         (cddddr entry))
            found)
      (setq n (+ n 1)))))

;;; The name of the function a frame is running: its global name, `(setf
;;; foo)`, `(flet foo)` or `(labels foo)` for a local function (as the
;;; compiler names them), or `lambda`. A generic function's frame is rarely
;;; seen, since it hands its arguments to its method by a tail call, which
;;; replaces the frame; what is seen is the method's, named `lambda`.
(defun frame-name (frame)
  (let ((function (frame-function frame)))
    (if (%generic-function-p function)
        (%gf-name function)
        (or (%function-name function) 'lambda))))

;;; The variables a frame's function can see that are its own, as
;;; `(name . value)`, innermost first: its parameters and what it has bound
;;; since, as they are now. Names the compiler made up (uninterned symbols,
;;; such as those `&optional` parameters are bound under first) are left
;;; out, and so is a name an inner binding hides.
(defun frame-locals (frame)
  (let ((seen nil) (shown nil))
    (dolist (pair (%frame-locals (frame-environment frame) (frame-function frame))
                  (nreverse shown))
      (unless (or (null (symbol-package (car pair))) (member (car pair) seen))
        (push (car pair) seen)
        (push pair shown)))))

;;; The call a frame keeps, as a form: `(name argument...)`.
(defun frame-call (frame)
  (cons (frame-name frame) (frame-arguments frame)))

;;; SBCL's: the calls of the frames below the caller, innermost first.
(defun list-backtrace (&key count)
  (mapcar #'frame-call (cdr (frames (and count (+ count 1))))))

;;; SBCL's: one line a frame, `0: (FOO 1 2)`, innermost first. Arguments are
;;; printed shallowly, since a frame may hold a structure too large or too
;;; circular to print whole.
(defun print-backtrace (&key count (stream *standard-output*))
  (let ((*print-level* 4) (*print-length* 10) (n 0))
    (dolist (call (cdr (mapcar #'frame-call (frames (and count (+ count 1))))))
      (format stream "~D: ~S~%" n call)
      (setq n (+ n 1)))
    (values)))

;;; SBCL's: unwind to FRAME — every `unwind-protect` cleanup inside it runs
;;; and every special binding made inside it is undone, as a `throw` past it
;;; would — then call THUNK, a function of no arguments, in the frame's
;;; place. THUNK's values are the frame's values, returned to whoever called
;;; the frame. A frame that has returned is a `control-error`, even when a
;;; later call of the same function has taken its place.
(defun unwind-to-frame-and-call (frame thunk)
  (unless (frame-p frame) (error 'type-error :datum frame :expected-type 'frame))
  (unless (functionp thunk) (error 'type-error :datum thunk :expected-type 'function))
  (%unwind-to-frame (frame-place frame) (frame-token frame) thunk))

;;; Whether NAME names a global function a frame can be retried through: a
;;; symbol or `(setf symbol)` naming a function, not a macro.
(defun %retry-by-name-p (name)
  (and (or (and (symbolp name) name (not (eq name 'lambda)))
           (and (consp name) (eq (car name) 'setf)))
       (fboundp name)
       (not (and (symbolp name) (or (macro-function name) (special-operator-p name))))))

;;; Run FRAME's call again, with the arguments it was first called with.
;;; Which function runs is SLIME's rule for SBCL: if the frame's function
;;; has a global name that names a function now, that function — so a
;;; function redefined while its frame waited runs as redefined, which is
;;; what fixing a function from inside an error and retrying it means —
;;; and otherwise the frame's own function, as for a `lambda`, a local
;;; function or a method, whose names, `lambda`, `(flet f)` and
;;; `(:method f ...)`, are not global ones. A method redefined while its
;;; frame waited is reached by retrying the frame that called its generic
;;; function, which dispatches afresh.
(defun restart-frame (frame)
  (let ((name (frame-name frame))
        (arguments (frame-arguments frame)))
    (let ((function (if (%retry-by-name-p name) (fdefinition name) (frame-function frame))))
      (unwind-to-frame-and-call frame (lambda () (apply function arguments))))))

;;; Return VALUES from FRAME, as if its function had returned them: SBCL's
;;; debugger command `return`.
(defun return-from-frame (frame &rest values)
  (unwind-to-frame-and-call frame (lambda () (values-list values))))
