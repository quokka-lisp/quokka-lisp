# Tutorial: Quokka Hop, a platform game

> *An imagined tutorial.* Quokka's environment is designed but not built
> (`docs/design.md`, *Environment and UI*). This tutorial pretends it
> exists, to find out what writing a small game would feel like. Every name
> from `quokka-ui`, `quokka-sound` and `quokka-stores` below is a proposal;
> the decisions it led to are in `docs/design.md`, *Games: frames, keys,
> sound and images*, and *Sound*.
> The Tower of Hanoi tutorial introduces viewers, drawing and commands, and
> the Rolodex tutorial introduces stores; read them first. The Common Lisp
> is real.

In this tutorial you will write *Quokka Hop*, a small platform game. A
quokka runs and jumps along a sunny island, collects gum leaves, keeps away
from snakes, and gets home to its burrow. The game has a title screen, an
options screen where the player chooses their own keys, the game itself, a
screen for being caught and one for getting home, sound effects for every
jump, bump, leaf and snake, and a tune for the title, for getting caught and
for getting home. Everything on the screen is drawn with rectangles,
circles, ellipses and lines; bitmaps come at the end, as an advanced step.

It takes an afternoon. Most of the work is your own code, which is the
point: the platform gives you a tile that is called every frame, keys as
they go down and up, shapes, and sounds, and the game is yours.

## 1. A system with two packages

```
New System hop
```

A game has two kinds of code that change for different reasons, the game
and its sounds, so give them a package each. In the system's definition:

```lisp
(define-system :hop
  (:packages :hop.sound :hop)
  (:main-view hop:hop-game))
```

and the two packages:

```lisp
(defpackage :hop.sound
  (:use :common-lisp :quokka-sound)
  (:export #:*jump* #:*bump* #:*collect* #:*hit*
           #:*title-song* #:*caught-song* #:*home-song* #:play-tune))

(defpackage :hop
  (:use :common-lisp :quokka-ui :quokka-stores :quokka-sound :hop.sound)
  (:export #:hop-game))
```

`quokka-sound` is a library, like `quokka-stores`: built on the platform,
knowing nothing about viewers or commands. Your game is where they meet.

## 2. A tile that is called every frame

A game is a loop: read the keys, move everything a little, draw it all,
again, sixty times a second. In Quokka the loop belongs to the platform. A
viewer declared `:every-frame` is called on every frame the screen shows,
and you write what happens in one frame:

```lisp
(define-viewer hop-game ()
  ((scene :accessor scene)
   (held :initform '() :accessor held-keys)
   (music :initform nil :accessor music))
  (:title "Quokka Hop")
  (:minimum-size 480 270)
  (:redisplay :every-frame))

(defmethod initialize-instance :after ((game hop-game) &key)
  (change-scene game (make-instance 'title-scene :game game)))

(defmethod viewer-tick ((game hop-game) seconds)
  (scene-tick (scene game) seconds))

(defmethod display-viewer ((game hop-game) stream)
  (with-drawing (stream :width 320 :height 180 :scale :fit)
    (scene-draw (scene game))))
```

Each frame the platform calls `viewer-tick` with the seconds since this
viewer's previous tick, then `display-viewer`. Keep them apart:
`viewer-tick` changes the world, `display-viewer` only draws it. This is not
just tidiness. Quokka redraws every visible viewer after any command, so if
you type in the Rolodex's search bar beside the game, the game gets extra
calls to `display-viewer`; if drawing moved the quokka, it would run faster
while you typed.

The time comes from the host, not from your program. The VM has no clock,
on purpose (chapter 2); the browser knows when it is about to show a frame,
and tells the world. Frames come about sixty times a second, more on some
screens and fewer on a busy computer, and after a pause `seconds` can be
large. Quokka passes it on as it is: what a game does with a long gap is
the game's business, and section 6 shows what this one does.

The drawing is 320 by 180, a small, chunky screen that `:scale :fit`
stretches to whatever size the tile is. Zoom the tile and the game fills
the screen.

## 3. Scenes

The title screen, the options, the game, being caught and getting home are
five different things on one tile. Each is a **scene**, an object that
knows how to tick, draw and take a key, and the game always has exactly
one:

```lisp
(defclass scene ()
  ((game :initarg :game :reader game)))

(defgeneric scene-enter (scene) (:method ((s scene)) nil))
(defgeneric scene-tick (scene seconds) (:method ((s scene) seconds) nil))
(defgeneric scene-draw (scene))
(defgeneric scene-key (scene key) (:method ((s scene) key) nil))

(defun change-scene (game scene)
  (when (music game)                 ; each scene brings its own music
    (stop (music game))
    (setf (music game) nil))
  (setf (scene game) scene)
  (scene-enter scene))
```

The default methods do nothing, so a title screen that never changes needs
no `scene-tick` at all. This is the pattern known as the *state pattern*:
the game's behaviour changes by replacing one object, and generic functions
choose the right code for whatever scene is current.

## 4. Keys, as they go down and up

Commands are right for buttons and menus, where a key means "do this once".
A game also needs to know which keys are *held*: the quokka runs for as long
as you hold the right arrow. A viewer sees every key that goes down or up
while it has the focus, through `viewer-input`:

```lisp
(defmethod viewer-input ((game hop-game) event)
  (let ((key (event-position event)))
    (case (event-kind event)
      (:key-down (unless (event-repeat-p event)
                   (pushnew key (held-keys game))
                   (scene-key (scene game) key)))
      (:key-up   (setf (held-keys game) (remove key (held-keys game)))))
    t))
```

Four things about keys are worth knowing.

1. A key has two names. `event-key` is the character it types, and
   `event-position` is where it is on the keyboard, such as `:key-a`,
   `:arrow-left` or `:space`. The key that types `a` on an English keyboard
   types `q` on a French one, but its position is the same, so a game uses
   positions and stays playable everywhere. `(key-name position)` gives the
   character that position types on this keyboard, for showing to the
   player.
2. Holding a key repeats its key-down. A text field wants the repeats; the
   game ignores them with `event-repeat-p`.
3. `viewer-input` returns true for the keys it used, and this one uses them
   all. A viewer that returns false lets the key go on to the viewer's
   command keys, which is how the Hanoi tutorial's arrow keys worked without
   any `viewer-input` at all. Keys held with Cmd or Ctrl belong to the
   platform and never arrive here, and Cmd-Escape always takes you back to
   the environment, so no game can trap you in it.
4. When the tile loses the focus, Quokka sends a key-up for every key still
   held. Without that, a key released while you were clicking elsewhere
   would stay held, and the quokka would run off on its own.

The game never asks for a key by its position, though. It asks for an
*action*, and the player decides which key means it:

```lisp
(defclass settings ()
  ((bindings :initform (list :left :arrow-left :right :arrow-right
                             :jump :space :back :escape)
             :accessor bindings))
  (:metaclass stored-class))

(defvar *settings-store* (make-store :journalled t))
(defvar *settings* (store-add *settings-store* (make-instance 'settings)))

(defun key-for (action) (getf (bindings *settings*) action))

(defun held-p (game action)
  (member (key-for action) (held-keys game)))
```

The settings live in a journalled store, so the player's keys survive a
crash and the next visit. The store is not private: an exported game ships
with the default keys you set here.

## 5. A level is a picture made of letters

The level is data, and the easiest data to design a level in is text you
can see:

```lisp
(defparameter *level*
  '("                                                            "
    "                                                            "
    "                                                            "
    "                                    L                       "
    "                       L          =====                     "
    "            L        =======                  L          B  "
    "          =====                ===          =====    ======="
    "   L                                                        "
    "Q      ==       S         S    L      S                S    "
    "=====================    ========================   ========"
    "=====================    ========================   ========"))
```

`=` is ground, `L` a gum leaf, `S` a snake, `Q` where the quokka starts and
`B` its burrow. Each letter is a 16-pixel square, a **tile**, and you can
redesign the level by typing. Eleven rows of 16 pixels fill the screen's
height; the gaps in the bottom two rows are where the island has holes. Reading it is a loop over rows and columns:

```lisp
(defconstant +tile+ 16)

(defclass thing ()
  ((kind :initarg :kind :reader kind)
   (x :initarg :x :accessor x)
   (y :initarg :y :accessor y)))

(defun read-level (rows)
  "The level's solid tiles, as a 2D array of booleans, and its things."
  (let ((solid (make-array (list (length rows) (length (first rows)))
                           :initial-element nil))
        (things '()))
    (loop for row in rows
          for ty from 0
          do (loop for char across row
                   for tx from 0
                   for kind = (case char (#\L :leaf) (#\S :snake)
                                         (#\Q :start) (#\B :burrow))
                   do (when (char= char #\=) (setf (aref solid ty tx) t))
                      (when kind
                        (push (make-instance 'thing :kind kind
                                                    :x (* tx +tile+)
                                                    :y (* ty +tile+))
                              things))))
    (values solid things)))
```

## 6. The quokka moves

Moving the quokka is physics in two lines: speed changes position, and
gravity changes speed. Real physics depends on how long each step is, and
frames are not all the same length, so a jump would go higher on a busy
computer. The cure is a **fixed time step**: the game always moves in steps
of exactly 1/120 of a second, as many as fit into the time that has passed,
and keeps the remainder for the next frame.

```lisp
(defconstant +step+ 1/120)
(defconstant +gravity+ 900)
(defconstant +run-speed+ 110)
(defconstant +jump-speed+ 320)

(defclass play-scene (scene)
  ((solid :accessor solid)
   (things :accessor things)
   (quokka :accessor quokka)
   (leaves :initform 0 :accessor leaves)
   (clock :initform 0 :accessor clock)
   (owed :initform 0 :accessor owed)))

(defclass quokka (thing)
  ((vx :initform 0 :accessor vx)
   (vy :initform 0 :accessor vy)
   (on-ground :initform nil :accessor on-ground)
   (facing :initform 1 :accessor facing)))

(defmethod scene-tick ((s play-scene) seconds)
  (incf (owed s) (min seconds 1/10))
  (loop while (>= (owed s) +step+)
        do (step-world s +step+)
           (decf (owed s) +step+)
           (incf (clock s) +step+)))
```

`+step+` is the ratio 1/120, exact, so 120 steps make exactly one second
however many frames they were spread over. `(min seconds 1/10)` stops a
long pause, such as the tab being hidden, from arriving as one enormous
step that sends the quokka through the floor.

One step reads the actions, moves the quokka across, then up or down, and
looks at what it touched:

```lisp
(defun step-world (s dt)
  (let ((q (quokka s))
        (game (game s)))
    (setf (vx q) (* +run-speed+ (- (if (held-p game :right) 1 0)
                                   (if (held-p game :left) 1 0))))
    (unless (zerop (vx q)) (setf (facing q) (signum (vx q))))
    (when (and (held-p game :jump) (on-ground q))
      (setf (vy q) (- +jump-speed+))
      (play *jump*))
    (incf (vy q) (* +gravity+ dt))
    (move-across s q (* (vx q) dt))
    (move-down s q (* (vy q) dt))
    (look-around s q)))
```

Moving one axis at a time keeps collisions simple: after moving across,
anything solid in the way was hit from the side; after moving down, from
above or below. The quokka is a 12 by 14 box, and a box is in something
solid if any of its corners is:

```lisp
(defconstant +width+ 12)
(defconstant +height+ 14)

(defun solid-at-p (s px py)
  (let ((tx (floor px +tile+)) (ty (floor py +tile+)))
    (and (array-in-bounds-p (solid s) ty tx)
         (aref (solid s) ty tx))))

(defun box-blocked-p (s x y)
  (or (solid-at-p s x y)
      (solid-at-p s (+ x +width+ -1) y)
      (solid-at-p s x (+ y +height+ -1))
      (solid-at-p s (+ x +width+ -1) (+ y +height+ -1))))

(defun move-across (s q dx)
  (incf (x q) dx)
  (when (box-blocked-p s (x q) (y q))
    (decf (x q) dx)
    (setf (vx q) 0)))

(defun move-down (s q dy)
  (incf (y q) dy)
  (setf (on-ground q) nil)
  (when (box-blocked-p s (x q) (y q))
    (cond ((plusp dy)                       ; landed on something
           (setf (y q) (- (* +tile+ (floor (+ (y q) +height+) +tile+)) +height+)
                 (on-ground q) t))
          (t                                ; hit its head
           (setf (y q) (* +tile+ (1+ (floor (y q) +tile+))))
           (play *bump*)))
    (setf (vy q) 0)))
```

What it touches decides the game:

```lisp
(defun touching-p (q thing)
  (and (< (abs (- (+ (x q) 6) (+ (x thing) 8))) 12)
       (< (abs (- (+ (y q) 7) (+ (y thing) 8))) 12)))

(defun look-around (s q)
  (dolist (thing (things s))
    (when (touching-p q thing)
      (case (kind thing)
        (:leaf   (setf (things s) (remove thing (things s)))
                 (incf (leaves s))
                 (play *collect*))
        (:snake  (play *hit*)
                 (change-scene (game s) (make-instance 'caught-scene :game (game s)))
                 (return-from look-around))
        (:burrow (change-scene (game s)
                               (make-instance 'home-scene :game (game s)
                                              :leaves (leaves s) :time (clock s)))
                 (return-from look-around)))))
  (when (> (y q) (* +tile+ (array-dimension (solid s) 0)))   ; fell off the island
    (play *hit*)
    (change-scene (game s) (make-instance 'caught-scene :game (game s)))))
```

## 7. Drawing with shapes

Everything is drawn from shapes. The world is wider than the screen, so the
drawing is moved left as the quokka runs right, with a translation, and the
score is drawn after it, where it stays put:

```lisp
(defun camera-x (s)
  (let ((world-width (* +tile+ (array-dimension (solid s) 1))))
    (max 0 (min (- (x (quokka s)) 150) (- world-width 320)))))

(defmethod scene-draw ((s play-scene))
  (draw-rectangle* 0 0 320 180 :ink +sky+)
  (with-translation ((- (camera-x s)) 0)
    (draw-ground s)
    (dolist (thing (things s)) (draw-thing (kind thing) thing))
    (draw-quokka (quokka s)))
  (draw-text* (format nil "Leaves ~D   ~,1F s" (leaves s) (float (clock s)))
              8 14 :ink +ink-text+))
```

A different `draw-thing` method for each kind, chosen by an `eql`
specializer on the keyword:

```lisp
(defgeneric draw-thing (kind thing))

(defmethod draw-thing ((kind (eql :leaf)) thing)
  (draw-ellipse* (+ (x thing) 8) (+ (y thing) 9) 5 3 :ink +leaf-green+)
  (draw-line* (+ (x thing) 3) (+ (y thing) 9) (+ (x thing) 13) (+ (y thing) 9)
              :ink +leaf-vein+))

(defmethod draw-thing ((kind (eql :snake)) thing)
  (let ((x (x thing)) (y (+ (y thing) 12)))
    (draw-polyline* (list x y (+ x 4) (- y 4) (+ x 8) y (+ x 12) (- y 4) (+ x 16) y)
                    :ink +snake+ :line-thickness 3)
    (draw-circle* (+ x 16) (- y 1) 2 :ink +snake+)))

(defmethod draw-thing ((kind (eql :burrow)) thing)
  (draw-ellipse* (+ (x thing) 8) (+ (y thing) 12) 9 6 :ink +burrow+))

(defmethod draw-thing ((kind (eql :start)) thing)
  (declare (ignore thing)))

(defun draw-quokka (q)
  (let ((cx (+ (x q) 6)) (cy (+ (y q) 8)) (f (facing q)))
    (draw-ellipse* cx cy 7 6 :ink +fur+)                    ; body
    (draw-circle* (+ cx (* f 5)) (- cy 5) 4 :ink +fur+)     ; head
    (draw-circle* (+ cx (* f 3)) (- cy 9) 2 :ink +ear+)     ; ear
    (draw-circle* (+ cx (* f 7)) (- cy 6) 1 :ink +eye+)))   ; eye
```

The inks are the game's own colours, defined once, with a version for the
dark theme:

```lisp
(define-ink +sky+        :light "#cfe8f7" :dark "#1d2b3a")
(define-ink +fur+        :light "#9a7650" :dark "#b48f66")
(define-ink +leaf-green+ :light "#3b8a3e" :dark "#5fb062")
;; and the others in the same way
```

## 8. Sound effects

Sounds go in `hop.sound`. Each is built from a few parts, an oscillator
that makes a tone and an envelope that shapes how it starts and fades,
and the host plays it; your program never makes the samples itself, because
the VM cannot promise to have them ready on time.

The work is split in two. The platform can do one thing with sound: play a
*graph* of a few kinds of parts (oscillators, noise, gain, a filter, a
delay, a pan, a buffer of samples) at a given moment on the audio clock,
and give back a playback you can stop. Everything else, `envelope`,
`glide`, `then`, note names, songs and instruments, is the `quokka-sound`
library, written in Lisp, building those graphs. You can read it, change it
and add to it, and nothing in the platform has to change.

```lisp
(in-package :hop.sound)

(defparameter *jump*
  (make-sound (envelope (glide (square 300) :to 600 :seconds 0.12)
                        :release 0.08 :gain 0.3)))

(defparameter *bump*
  (make-sound (envelope (noise) :release 0.05 :gain 0.3)))

(defparameter *collect*
  (make-sound (then (envelope (sine 880) :release 0.06)
                    (envelope (sine 1320) :release 0.12))))

(defparameter *hit*
  (make-sound (envelope (glide (sawtooth 440) :to 110 :seconds 0.4)
                        :release 0.1 :gain 0.3)))
```

`glide` slides an oscillator's pitch, `then` plays one sound after another.
Try them in the listener with `(play *jump*)`, and each prints as a
waveform you can click to hear again. Change a number and press Cmd-Enter,
and the next jump in the running game sounds different.

## 9. Music

A tune is data too: a list of notes, each a pitch and a length, where a
length is a fraction of a bar. Lisp's ratios make this read like a score:

```lisp
(defparameter *title-song*
  '((:c5 1/8) (:e5 1/8) (:g5 1/8) (:c6 1/4) (:g5 1/8) (:a5 1/4) (:rest 1/8)
    (:f5 1/8) (:a5 1/8) (:c6 1/8) (:g5 1/2)))

(defparameter *caught-song*
  '((:g4 1/8) (:f4 1/8) (:e4 1/8) (:d4 1/4) (:c4 1/2)))

(defparameter *home-song*
  '((:c5 1/16) (:e5 1/16) (:g5 1/16) (:c6 1/8) (:rest 1/16)
    (:g5 1/16) (:c6 1/2)))

(defun chip (frequency)
  "The instrument the game's tunes are played on."
  (envelope (square frequency) :attack 0.01 :release 0.08 :gain 0.15))

(defun play-tune (song &key loop)
  (play-song song :instrument #'chip :tempo 132 :loop loop))
```

`play-song` returns a **playback**, which you keep in order to stop, pause
or resume it. The library does not hand the whole tune to the host at once.
A process of its own wakes every fiftieth of a second or so, asks the host
for the audio clock, and hands over the notes due in the next tenth of a
second, which is called a *lookahead scheduler*: a slow frame in the game
delays when Lisp asks for the next notes, not when they sound, so the music
never stutters.

### When nobody is looking

Switch to another desktop while the game runs, or zoom another tile over
it, or put the browser tab in the background, and the game should stop:
nobody wants a snake to catch the quokka while they are reading code.
Quokka ticks only viewers that are *running*, visible and not stopped by
the inspector, so the world stops by itself. The music does not, because it
belongs to the game and not to the tile. Quokka tells the game when it
stops and starts running, and the game decides what to do:

```lisp
(defmethod viewer-running-changed ((game hop-game) running)
  (when (music game)
    (if running
        (resume (music game))
        (pause (music game)))))
```

When the game runs again, its first tick brings the whole time it was
away, which is what section 6's `(min seconds 1/10)` is for.

## 10. The screens

The title screen starts its song when it is entered and waits for a key:

```lisp
(in-package :hop)

(defclass title-scene (scene) ())

(defmethod scene-enter ((s title-scene))
  (setf (music (game s)) (play-tune *title-song* :loop t)))

(defmethod scene-draw ((s title-scene))
  (draw-rectangle* 0 0 320 180 :ink +sky+)
  (draw-text* "Quokka Hop" 160 70 :align :center :text-style '(:sans :bold :huge))
  (draw-text* (format nil "~A to start · O for options"
                      (key-name (key-for :jump)))
              160 110 :align :center))

(defmethod scene-key ((s title-scene) key)
  (cond ((eql key (key-for :jump))
         (change-scene (game s) (make-instance 'play-scene :game (game s))))
        ((eql key :key-o)
         (change-scene (game s) (make-instance 'options-scene :game (game s))))))
```

`play-scene` reads the level when it is entered:

```lisp
(defmethod scene-enter ((s play-scene))
  (multiple-value-bind (solid things) (read-level *level*)
    (let ((start (find :start things :key #'kind)))
      (setf (solid s) solid
            (things s) things
            (quokka s) (make-instance 'quokka :kind :quokka
                                              :x (x start) :y (y start))))))
```

The options screen lists the actions and waits for a key when you choose
one. The change goes through the store in a transaction, so it is
journalled and the player's keys are kept:

```lisp
(defclass options-scene (scene)
  ((choice :initform 0 :accessor choice)
   (waiting :initform nil :accessor waiting)))

(defparameter *actions* '(:left :right :jump :back))

(defmethod scene-draw ((s options-scene))
  (draw-rectangle* 0 0 320 180 :ink +sky+)
  (draw-text* "Keys" 160 30 :align :center :text-style '(:sans :bold :large))
  (loop for action in *actions*
        for i from 0
        for y from 60 by 20
        do (draw-text* (format nil "~:(~A~)" action) 100 y
                       :ink (if (= i (choice s)) +ink-accent+ +ink-text+))
           (draw-text* (if (and (waiting s) (= i (choice s)))
                           "press a key…"
                           (key-name (key-for action)))
                       200 y)))

(defmethod scene-key ((s options-scene) key)
  (cond ((waiting s)
         (let ((bindings (copy-list (bindings *settings*))))
           (setf (getf bindings (nth (choice s) *actions*)) key)
           (with-transaction (*settings-store*)
             (setf (bindings *settings*) bindings)))
         (setf (waiting s) nil))
        ((eql key :arrow-down) (setf (choice s) (mod (1+ (choice s)) 4)))
        ((eql key :arrow-up)   (setf (choice s) (mod (1- (choice s)) 4)))
        ((eql key :enter)      (setf (waiting s) t))
        ((eql key (key-for :back))
         (change-scene (game s) (make-instance 'title-scene :game (game s))))))
```

Why copy the list instead of writing `(setf (getf (bindings *settings*)
action) key)` inside the transaction? Because when the action is already
in the list, `setf` of `getf` may change the list where it stands, and the
slot `bindings` is never written. A stored class notices writes to its
slots, not changes deep inside the objects they hold, so that change would
not be journalled, and the player's keys would be lost in a crash. Making a
new list and writing the slot is the reliable way: the store sees one write,
of a value nobody else shares.

The arrow keys and Enter that move through the options are fixed, so that a
player who maps *jump* to the down arrow can still find their way back.

Being caught plays its song and offers another go; getting home plays the
other and shows the time:

```lisp
(defclass caught-scene (scene) ())

(defmethod scene-enter ((s caught-scene))
  (setf (music (game s)) (play-tune *caught-song*)))
(defmethod scene-draw ((s caught-scene))
  (draw-rectangle* 0 0 320 180 :ink +dusk+)
  (draw-text* "Caught!" 160 80 :align :center :text-style '(:sans :bold :huge))
  (draw-text* "Any key to try again" 160 115 :align :center))
(defmethod scene-key ((s caught-scene) key)
  (declare (ignore key))
  (change-scene (game s) (make-instance 'play-scene :game (game s))))

(defclass home-scene (scene)
  ((leaves :initarg :leaves :reader leaves)
   (time :initarg :time :reader home-time)))

(defmethod scene-enter ((s home-scene))
  (setf (music (game s)) (play-tune *home-song*)))
(defmethod scene-draw ((s home-scene))
  (draw-rectangle* 0 0 320 180 :ink +sky+)
  (draw-text* "Home!" 160 70 :align :center :text-style '(:sans :bold :huge))
  (draw-text* (format nil "~D leaves in ~,1F seconds" (leaves s) (float (home-time s)))
              160 105 :align :center)
  (draw-text* "Any key for the title" 160 130 :align :center))
(defmethod scene-key ((s home-scene) key)
  (declare (ignore key))
  (change-scene (game s) (make-instance 'title-scene :game (game s))))
```

## 11. Playing it, and changing it while you play

```
Start Application hop
```

The game opens on a desktop of its own. Click the tile and press the key
the title screen names. While you play, open the listener on another
desktop, change `+jump-speed+` to 360 and press Cmd-Enter: the next jump is
higher, without restarting. Change the level's text, and the next time you
start it the level is new. Change `*jump*` and the next jump sounds
different. Nothing stops, because the game is just objects in the world,
and you are changing them.

Right-click the quokka while the game runs. The tile stops running, so the
game pauses and its music with it, and the inspector shows the quokka: its position, its speed,
whether it is on the ground. Change `vy` to -500 in the inspector and
continue: the quokka leaps.

## 12. Advanced: sprites

Shapes keep the game small and easy to change. A game with drawn
characters uses **sprites**, small pictures made of pixels. Quokka can make
an image from text the way the level is made, one character per pixel:

```lisp
(defparameter *quokka-sprite*
  (make-image '("....##.."
                "...####."
                "..##o###"
                ".#######"
                "########"
                "########"
                ".##..##.")
              :palette (list #\# +fur+ #\o +eye+)))

(defun draw-quokka (q)
  (draw-image* *quokka-sprite* (x q) (y q) :flip (minusp (facing q))))
```

or load one from a PNG file through a file host, with
`(load-image #p"browser:/hop/quokka.png")`. An image is a value in the
world like any other, so it is saved with the world and exported with the
game. `:scale :fit` scales sprites too; for pixel art, `:smooth nil` keeps
the pixels square.

What an image holds is bytes, in an array of element type
`(unsigned-byte 8)`, one byte to an element: the PNG file exactly as it
was read, or the colours of the pixels `make-image` worked out. Decoding a
PNG and drawing it is the host's work, because the browser does it quickly
and well. A program that wants to change pixels, such as a sprite editor,
asks for them with `(image-pixels image)`, gets a byte array of red, green,
blue and alpha for every pixel, changes it like any array, and makes a new
image from it. `make-image` from strings, sprite sheets and animation frames
are library functions written in Lisp; the platform only draws.

## 13. Keeping it and giving it away

Commit, and export it:

```
Export System hop As Web Page :locked
```

Your friend gets the title screen, the options, the game and the music,
and the default keys. Their own keys and best times stay in their browser.

## Try next

- Keep the best time in a journalled store, and show it on the title
  screen.
- Record every key that goes down or up, with the frame it happened in,
  and play a run back. Because the game moves in fixed steps and the world
  has no clock of its own, the same keys give exactly the same run.
- A second level, chosen when you get home.
- Background music during the game that speeds up when the quokka is close
  to a snake.
