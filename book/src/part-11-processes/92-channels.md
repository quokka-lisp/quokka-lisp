# Channels

> *Where this fits.* Chapter 91 gave programs threads, and the usual tools
> for threads that share data: mutexes and wait queues. This chapter adds
> the other way of writing concurrent programs, the one Go made popular:
> threads that share nothing they don't send each other, over *channels*.
> It is also the first layer of a plan for using more than one processor,
> which the end of the chapter describes, and which keeps the same names
> when a channel's other end moves to another core.

*Status: done, issue #34 (layer 1 of the plan decided there). Layer 2, the
worker pool, is designed and not built (#132).*

## Concurrency is not parallelism

Chapter 90's processes take turns on one processor, so they never make a
program faster. When we asked what they are for, Rob Pike's distinction
answered it. *Concurrency* is structuring a program as several things in
progress at once, and needs only one processor. *Parallelism* is doing
several things at the same instant, and needs several. Green threads give
the first. They are for waiting: three requests to the outside world, each
taking a tenth of a second, take a tenth of a second together, because the
waits overlap. And they are for staying alive while something long happens:
a picture that draws itself row by row while the listener still answers, or
a game that keeps running while you redefine the function that makes the
quokka jump.

## Talking instead of sharing

With mutexes, threads share variables and take turns holding a lock around
every use. Go's motto turns that round: *don't communicate by sharing
memory; share memory by communicating.* A channel is a queue between
threads. One thread sends a value, another receives it, and only the
receiver uses it afterwards, so there is nothing to lock.

```lisp
(let* ((ch (make-channel))
       (th (make-thread (lambda () (send ch :hello) :sent))))
  (receive ch))            ; => :HELLO
```

A channel made without a capacity is a meeting place: `send` waits until a
receiver has taken the value, so the sender knows its value arrived. With a
capacity it holds that many values, and `send` waits only when it is full.
`close-channel` says nothing more is coming: receivers get what is left,
then `nil` with a second value of `nil`, as Go's `v, ok := <-ch` has it.

`select` waits on several channels at once and takes the first that can go:

```lisp
(select ((receive results v) (show v))
        ((receive cancel v) (stop))
        (otherwise (keep-drawing)))
```

Go chooses at random among the clauses that are ready, so that no channel
is starved. Quokka takes the first ready clause in the order written,
because the scheduler of chapter 90 is deterministic and we wanted a
program with channels to run the same way twice.

## Tasks

Most uses of a thread are a function whose value somebody wants later.
`spawn-task` starts one and answers at once with a channel that will receive
the value, which Go programmers write by hand and other languages call a
*future*:

```lisp
(let ((tasks (let ((ts nil))
               (dotimes (y height (nreverse ts))
                 (push (spawn-task 'mandel-row y width height limit) ts)))))
  (mapcar #'receive tasks))
```

That computes a Mandelbrot picture one row a task, and a test checks that it
gives exactly the rows the plain loop gives. If a task signals an error,
nobody is waiting for it in the task's own thread. So the task catches it,
and `receive` signals it again in the thread that wanted the value, which is
what Clojure's futures do.

## How it is built

Everything is Lisp, on chapter 91's threads. Our first version gave each
channel a mutex and a queue of values, and an unbuffered sender put its
value in the queue and then watched a flag to see whether it had been
taken. A reviewer who had not written it tried it under the scheduler and
reproduced six faults. The one that showed what was wrong was closing a
channel while a sender waited. The close and the sender's look at its flag
were not one step, so in 65 of 200 timings the value was delivered *and*
the sender was told it had failed. A sender and a receiver that both used
`select` waited for each other for ever.

So we used the algorithm Go's own runtime uses. Every operation is one
critical section. It either completes at once, handing its value straight
to a waiting partner or through the buffer, or it puts a *waiter* in the
channel's queue of waiting senders or receivers, and then waits. A waiter
is completed only by its partner, under the lock, so the waiting thread
watches one flag that a partner sets once (or that the thread itself sets
to say it has stopped waiting):

```lisp
(defun send (channel value)
  (let* ((w nil)
         (result (%with-channels
                   (or (%try-send channel value)
                       (progn (setq w (%make-waiter value nil nil))
                              (%queue-push (channel-senders channel) w)
                              :wait)))))
    (when (eq result :wait)
      (%wait-for w)
      (setq result (if (%waiter-ok w) :sent :closed)))
    (when (eq result :closed)
      (error 'closed-channel-error :channel channel))
    value))
```

Closing a channel completes every waiter, a receiver with nothing and a
sender with a refusal, in the same critical section. A `select` puts one
waiter on each of its channels, all sharing a *selection*; the first partner
to complete one of them completes the select, and the others are passed
over. There is one lock for all channels, so that a select takes it once
for all of its channels and two selects can never each hold one channel the
other wants. On one processor nobody holds it for more than a few steps.
Each of the six faults is now a test.

A second review, of the new version, found the faults that were left, and
they were about ending threads. A thread ended while letting go of the
channels' lock left the lock held for ever. One ended in the moment between
putting its waiter in a queue and setting up the cleanup that would take
it out again left a waiter nobody would ever wake, so a value sent to it
was lost. The cure for both was chapter 90's: ending a thread waits until
it is safe. Each operation, with its cleanup, is now a region in which the
thread cannot be ended, and only its waiting is interruptible. Common Lisp has no channels, so there
was no standard to follow. The nearest library, ChanL, has the same model,
and its names differ from these only in `recv`.

## The second layer

The plan we decided on #34 uses the same names across processors. One
world stays on one operating-system thread for good, so its collector never has to deal
with another thread touching its heap; that is what makes Go's runtime
large. For parallel work there would be a *pool of worker worlds*, one per
core, each a heap of its own with its own green threads and its own
collector, sharing nothing:

1. A task sent to the pool is a function's *name* and its arguments, which
   are copied, since a closure cannot cross into another heap.
2. Its value comes back over a channel that copies it.
3. A worker that meets a function it does not have asks the main world for
   its definition, which chapter 84 keeps, so a function written a moment
   ago at the REPL is not missing in the workers.

We measured what that would cost. A world is 2 MB and ready in 20 ms, and a
pool of eight is started once. The Mandelbrot showed how work should be
split: its rows differ twentyfold in cost, so eight bands of three rows
would give at most a 3.6-fold speedup on eight cores. Twenty-four row tasks
handed to whichever worker is free would give nearly eightfold. With
`spawn-task` and `receive` the program is the same in both layers. That is
why `spawn-task` already accepts a function's name, though in one world a
closure would do.

## What did it cost?

No Rust. `channels.lisp` and two tests. One checks:
- an unbuffered send waiting for its receiver;
- a full buffer making its sender wait;
- a closed channel;
- `select` with and without `otherwise`;
- tasks, and a task's error;
- the Mandelbrot computed both ways.

The other holds the six faults the review found, including the close falling
at sixty different moments while a sender waits.

## What is still missing?

The second layer, all of it. And `receive` takes one value; a task's other
values are dropped.
