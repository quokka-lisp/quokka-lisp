;;; block, return-from, catch, throw, tagbody, go, unwind-protect.
;;;
;;; Leaving a computation from the middle of it. Everything here is behaviour
;;; the standard specifies precisely and that I guessed at while implementing.

;; block is lexical: return-from reaches a block that encloses it in the source.
(block nil 1 2 3)
(block nil 1 (return-from nil 2) 3)
(block done (return-from done 'early) 'late)
(block outer (block inner (return-from outer 'far)) 'near)
(block outer (block inner (return-from inner 'near)) 'after)

;; The value form is evaluated before the unwind.
(block b (return-from b (+ 1 2)))
(block b (return-from b))

;; A block around a function call still catches a return from inside it.
(defun thrower () 'normal)
(block b (thrower))

;; catch is dynamic: throw finds the nearest matching catch at run time, which
;; may be in a caller the thrower has never heard of.
(catch 'tag 1 2 3)
(catch 'tag (throw 'tag 42) 'unreached)
(catch 'a (catch 'b (throw 'a 'through)))
(catch 'a (catch 'b (throw 'b 'inner)))
(catch 'tag (throw 'tag (+ 20 22)))

(defun deep (n) (if (= n 0) (throw 'out 'arrived) (deep (- n 1))))
(catch 'out (deep 50))

;; tagbody is the primitive loop: labels and jumps. It yields nil.
(tagbody)
(tagbody a b c)
(setq n 0)
(tagbody top (setq n (+ n 1)) (if (< n 5) (go top)))
n
(setq out nil)
(tagbody (go skip) (setq out 'wrong) skip (setq out 'right))
out

;; Counting down, which is what every looping macro expands into.
(setq total 0)
(setq i 10)
(tagbody loop (if (= i 0) (go done)) (setq total (+ total i)) (setq i (- i 1)) (go loop) done)
total

;; unwind-protect runs its cleanup however the body ends.
;;
;; The accumulator is called `events` and not `log`, because CL:LOG is the
;; logarithm — a Common Lisp symbol, which SBCL's package lock refuses to let a
;; test bind. It does not look like one, which is why scripts/check-cl-names.lisp
;; now checks rather than trusting anyone to remember.
(setq events nil)
(unwind-protect 'body (setq events 'cleaned))
events

(setq events nil)
(block b (unwind-protect (return-from b 'left) (setq events 'cleaned)))
events

(setq events nil)
(catch 'x (unwind-protect (throw 'x 'thrown) (setq events 'cleaned)))
events

;; The cleanup runs while the unwind passes through, and the unwind carries on
;; afterwards to a catch further out.
(setq events nil)
(catch 'outer (unwind-protect (catch 'inner (throw 'outer 'past-both)) (setq events 'ran)))
events

;; Cleanups nest, innermost first.
(setq events nil)
(catch 'x
  (unwind-protect
      (unwind-protect (throw 'x 'done) (setq events (cons 'inner events)))
    (setq events (cons 'outer events))))
events

;; The body's value survives the cleanup, and the cleanup's value is discarded.
(unwind-protect 'kept 'discarded)

;; Nowhere to go.
(return-from nowhere 1)
(throw 'nothing 1)
(go nowhere)
(block)
(catch)
