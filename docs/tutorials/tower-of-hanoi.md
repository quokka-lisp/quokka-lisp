# Tutorial: the Tower of Hanoi

> *An imagined tutorial.* Quokka's environment is designed but not built
> (`docs/design.md`, *Environment and UI*; the book's *The environment, on
> paper*). This tutorial pretends it exists, to find out what writing a small
> graphical program would feel like. Every name from `quokka-ui` below is a
> proposal, and the decisions it led to are in `docs/design.md`, *Drawing,
> commands and redisplay*. The Common Lisp is real.

In this tutorial you will write a program that solves the Tower of Hanoi and
shows the solution in a tile of its own, one move at a time, with a *Back*
and a *Forward* button. It takes about forty minutes. You will meet systems
and packages, a recursive solution, a viewer of your own, Quokka's drawing
functions, commands that become buttons, and changing a program while it is
on the screen.

The puzzle, if you have not met it: three pegs, and a stack of disks on the
first, each smaller than the one below. Move the whole stack to the third
peg, one disk at a time, never putting a disk on a smaller one.

## 1. A system and an empty package

Everything you write in Quokka lives in a package, and a program of more
than a few lines gets a system of its own to hold its packages. In the
browser, choose *New system* at the bottom of the systems column, or type
the command in the command line at the bottom of the screen:

```
New System hanoi
```

Quokka makes a system called `hanoi` with one empty package, also called
`hanoi`, and opens the package's own definition in an editor. Change it to
say what the package uses and what it offers to others:

```lisp
(defpackage :hanoi
  (:use :common-lisp :quokka-ui)
  (:export #:hanoi-viewer #:solve))
```

Press Cmd-Enter. The definition is installed, the listener's prompt changes
to `hanoi 1 ›`, and you are ready to write the first function.

## 2. Solving it in the listener

The solution is three sentences long, and the third is where the recursion
is. To move *n* disks from one peg to another: move the *n − 1* disks above
the largest out of the way, onto the spare peg; move the largest disk; then
move the *n − 1* disks from the spare peg onto it. Moving *n − 1* disks is
the same problem, smaller, and moving none is nothing at all.

In the browser, choose *Function* under *Add a definition*, and write:

```lisp
(defun solve (n &optional (from 0) (to 2) (via 1))
  "The moves that take N disks from peg FROM to peg TO, as (FROM . TO) pairs."
  (if (zerop n)
      '()
      (append (solve (1- n) from via to)
              (list (cons from to))
              (solve (1- n) via to from))))
```

Cmd-Enter installs it. Try it in the listener:

```
hanoi 2 › (solve 3)
((0 . 2) (0 . 1) (2 . 1) (0 . 2) (1 . 0) (1 . 2) (0 . 2))
```

Seven moves, and the fourth, `(0 . 2)`, is the largest disk going home with
the other two parked on the middle peg. `(length (solve 10))` is 1023:
every disk you add doubles the work and adds one.

The result is a presentation, so you can right-click it and choose *Inspect*
to see it as a table, or drag it into the editor. It is the same list.

## 3. From moves to positions

To show the puzzle we need positions, not moves: where every disk is after
each move. A position is three lists, one per peg, the top disk first.
Disks are numbered from 1, the smallest, to *n*.

```lisp
(defun start-position (n)
  "All N disks on the first peg."
  (vector (loop for disk from 1 to n collect disk) '() '()))

(defun apply-move (pegs move)
  "The position after MOVE, a (FROM . TO) pair. PEGS is not changed."
  (let ((pegs (copy-seq pegs))
        (disk (first (aref pegs (car move)))))
    (setf (aref pegs (car move)) (rest (aref pegs (car move))))
    (push disk (aref pegs (cdr move)))
    pegs))

(defun positions (n)
  "Every position from the start to the end, one more than there are moves."
  (let ((pegs (start-position n)))
    (cons pegs
          (loop for move in (solve n)
                collect (setf pegs (apply-move pegs move))))))
```

`apply-move` copies the vector and never changes a list, so every position
in the result is its own and stepping back is just looking at an earlier
one. Check the last position:

```
hanoi 3 › (car (last (positions 3)))
#(NIL NIL (1 2 3))
```

## 4. A viewer of your own

A tile on the screen shows a **viewer**. The window manager knows only a few
things about one: its title, how small it can usefully be, and how to ask it
to draw itself. Everything else is yours. `define-viewer` defines a class of
viewer, with slots like any `defclass`:

```lisp
(define-viewer hanoi-viewer ()
  ((disks :initarg :disks :initform 4 :reader disks)
   (step :initform 0 :accessor current-step)
   (all-positions :reader all-positions))
  (:minimum-size 320 220))

(defmethod initialize-instance :after ((v hanoi-viewer) &key)
  (setf (slot-value v 'all-positions)
        (coerce (positions (disks v)) 'vector)))

(defmethod viewer-title ((v hanoi-viewer))
  (format nil "Tower of Hanoi · move ~D of ~D"
          (current-step v) (1- (length (all-positions v)))))
```

`:minimum-size` matters because tiles share the screen. When the window
manager splits a tile or you drag a border, it will not make this one
smaller than 320 by 220, so the pegs never get squashed.

`define-viewer` is a small convenience and nothing more. Ask the listener
what it means:

```
hanoi 5 › (macroexpand-1 '(define-viewer hanoi-viewer () ((disks :initform 4)) (:minimum-size 320 220)))
(PROGN (DEFCLASS HANOI-VIEWER (VIEWER) ((DISKS :INITFORM 4)))
       (DEFMETHOD VIEWER-MINIMUM-SIZE ((V HANOI-VIEWER)) (VALUES 320 220)))
```

A viewer is a class whose superclass is `viewer`, and each option is a
method on one of the functions the window manager calls. Anything you can
say with an option you could also say with a `defmethod` of your own.

## 5. Drawing

A viewer draws itself in `display-viewer`, a generic function you write a
method for. A viewer's own coordinates are logical pixels, one to a CSS
pixel whatever the screen, so that text and controls keep their size when a
tile grows. A picture wants the opposite, and `with-drawing` gives it a box
of its own: here 300 wide and 180 high, with the origin at the top left and
*y* growing downwards. `:scale :fit` fits that box to the tile and keeps its
proportions, which is why zooming the tile later needs no code. Clicks are
mapped back through the same scaling, so a disk is found where you click it
at any size.

```lisp
(defparameter *disk-inks*
  (list +coral+ +amber+ +teal+ +blue+ +purple+ +pink+))

(defun peg-x (peg) (+ 55 (* peg 95)))

(defun draw-disk (stream disk n x level)
  (let ((half-width (+ 10 (* 34 (/ disk n))))
        (bottom (- 160 (* level 14))))
    (with-output-as-presentation (stream disk 'disk)
      (draw-rectangle* (- x half-width) (- bottom 12) (+ x half-width) bottom
                       :ink (nth (mod (1- disk) 6) *disk-inks*)
                       :corner-radius 3))))

(defmethod display-viewer ((v hanoi-viewer) stream)
  (let ((pegs (aref (all-positions v) (current-step v)))
        (n (disks v)))
    (with-drawing (stream :width 300 :height 180 :scale :fit)
      (draw-rectangle* 10 160 290 166 :ink +structure+)
      (dotimes (peg 3)
        (let ((x (peg-x peg)))
          (draw-rectangle* (- x 2) 50 (+ x 2) 160 :ink +structure+)
          (loop for disk in (reverse (aref pegs peg))
                for level from 0
                do (draw-disk stream disk n x level)))))))
```

Four things to notice.

1. The drawing functions end in a star, `draw-rectangle*`, because they take
   coordinates as separate numbers. The versions without a star take point
   objects. The convention is CLIM's, which Quokka's drawing follows.
2. `(/ disk n)` is a ratio, 1/4 for the smallest of four disks. Drawing
   takes any real number, so you never have to round.
3. Inks such as `+coral+` and `+structure+` are named colours from the theme,
   not fixed values. In the dark theme they come out darker, and you did
   nothing for it.
4. `with-output-as-presentation` makes each disk a presentation of its
   number. Right-click a disk on the screen and *Inspect* shows `3`, and any
   command that wants a disk will accept a click on one.

## 6. Commands, buttons and keys

`display-viewer` draws only the puzzle. The Back and Forward buttons are
commands, and a command is an ordinary function with a little more said
about it:

```lisp
(defun last-step (v) (1- (length (all-positions v))))

(define-viewer-command (step-forward :viewer hanoi-viewer
                                     :label "Forward" :keys (#\Right) :bar t
                                     :when (< (current-step v) (last-step v)))
    (v)
  (incf (current-step v)))

(define-viewer-command (step-back :viewer hanoi-viewer
                                  :label "Back" :keys (#\Left) :bar t
                                  :when (plusp (current-step v)))
    (v)
  (decf (current-step v)))
```

`:bar t` puts the command in the tile's command bar, which the window
manager draws in the tile's frame, so every program's buttons sit in the
same place. `:when` says when the command makes sense: at the last move
Forward is shown as not applying, the right arrow does nothing, and typing
`Step Forward` says why. The same command can be run four ways: its button,
its key while the tile has the focus, the command line, and the menu you get
by right-clicking the tile. A button that belongs inside the drawing itself
is placed with `command-button`; Hanoi has none.

After any command, Quokka redraws every visible viewer. It does not draw
everything again: it compares the new drawing with the last one, record by
record, and changes only what differs, which after one move is one disk.
This is *incremental redisplay*, and it is why you can write
`display-viewer` as if it drew from scratch every time. The disk jumps to
its new place; making it travel there is in *Try next*.

## 7. Opening the tile

In the listener:

```
hanoi 4 › (open-viewer 'hanoi-viewer :disks 4)
#<HANOI-VIEWER Tower of Hanoi · move 0 of 15>
```

A new tile appears. Quokka has split the tile that had the focus, the
listener, along its longer side, and put the Hanoi viewer in the new half.
Press the right arrow fifteen times, or click *Forward*, and watch the disks
go across. Click *Back* to see a move again.

Back and Forward are in the tile's command bar, next to the standard
decorations every tile gets: *Zoom*, *Split* and *Close*. Click *Zoom* and the tile fills the screen, the drawing scaled
up because of `:scale :fit`; click it again and the tile returns to exactly
where it was. You wrote no code for either.

## 8. Changing it while it is on the screen

Every disk a different colour is cheerful but tells you nothing. Shade the
disks by size instead: change `draw-disk` to take its ink from the disk's
size, and press Cmd-Enter.

```lisp
:ink (shade +teal+ (/ disk n))
```

The tile redraws at once, at the same step, with the same disks in the same
places. Quokka knows which function drew which part of the screen, so when
you install a new `draw-disk` it redraws the viewers it drew. Nothing was
restarted, and the viewer object, with its step, is the one you had.

## 9. Keeping it

The browser marks `draw-disk` as changed since your last commit. When you
like the program, commit it from the history viewer with a message such as
*Tower of Hanoi, first version*. To give the program to someone who does not
use Quokka, name its main view in the system and export it as a web page:

```
Set System Main View hanoi hanoi-viewer
Export System hanoi As Web Page :locked
```

They get a page that opens straight into the puzzle, with the Back and
Forward buttons, and nothing else of the environment.

## Try next

- Make the disk travel: up its peg, across, and down the other. Declare the
  viewer `(:redisplay :every-frame)`, so that `display-viewer` runs on every
  frame the host announces, and keep the journey in a slot of its own, say
  `flight`, with the disk, the two pegs and how far it has got. Forward
  still sets the step at once and starts a flight; `display-viewer` draws
  the moving disk where the flight says, and the flight ends when it
  arrives. The step and the flight are kept apart on purpose: while disk 1
  is in the air the step is already 8, so Back, the inspector and saving
  the world all see a true position, never a half-made move.
- A *Play* command that steps forward by itself, twice a second. It needs a
  process of its own, so that the listener still answers while it plays;
  the process calls `(redisplay v)` after each step, and the time between
  steps comes from the host, never from the program.
- A sound on every move, pitched by the disk's size: `(play (pluck (disk-pitch
  disk)))`, with `pluck` built from an oscillator and an envelope.
- A slider for the number of disks. Fifteen disks is 32,767 moves; a step
  counter that is a presentation can take you to any of them.
