# Output records

> *Where this fits.* Chapter 94 planned a protocol in which the world keeps
> what is on the screen as objects and tells the page what changed, and
> chapter 95 made streams a class a program can define. This chapter builds
> the world's half: output records, the stream that makes them, the queue
> of messages that describes them, viewers that draw themselves again, and
> the command loop that waits for the person. The next chapter draws all of
> it in a browser.

*Status: done, issues #138 and #139, steps D and E of phase 4.*

## Records

When a program writes `(format t "~D cards" 3)` to a viewer, what it writes
becomes an object in the world: a **text record**, holding the string
`"3 cards"`, inside a **block record** that holds the viewer's contents,
inside the viewer's own record. Each record has a number, its identity,
which no other record in the world will ever have, and that number is all
the page knows it by. CLIM, the Common Lisp Interface Manager, called these
*output records*, and our records are CLIM's in spirit: the record tree is
the truth about what is on the screen, and the screen is drawn from it.

There are four kinds so far. A text record is a run of text in one style. A
block holds other records stacked downwards. An editor holds text the person
edits, and a viewer record is the root of a viewer's tree. They are CLOS
classes in `QUOKKA-UI`, because phase 5 will subclass them: a presentation
is a record that also knows the object it shows.

Writing makes records through a Gray stream, `record-stream`, which appends
text records to a block. It extends the last text record when the style has
not changed, so the seven writes `format` makes for `"~D cards"` become one
record, and starts a new one when it has:

```lisp
(defmethod stream-write-string ((stream record-stream) string &optional (start 0) end)
  (let ((end (or end (length string))))
    (when (< start end)
      (%extend-text (%stream-run stream) string start end)
      (let ((newline (position #\Newline string :start start :end end :from-end t)))
        (setf (%stream-column stream)
              (if newline (- end newline 1) (+ (%stream-column stream) (- end start)))))))
  string)
```

Styles and inks are names. A text style is CLIM's triple of family, face and
size, `(:fixed :bold nil)`; an ink is a name the theme gives a colour,
`:accent` or `:error`. `(with-ink (stream +error+) ...)` writes in the error
colour, and the world never says which colour that is: a dark theme makes it
lighter, a light one darker.

## Telling the page

The page has to learn about every record and every change, and the obvious
way, a message each time something changes, is wasteful in an instructive
way. `format` writes a line a few characters at a time; a message for each
write would describe the same record over and over, and each would be a
host request, a stop of the machine. So a change does not make a message.
It makes a *note* in a queue: this record was added there, this one's text
changed. When the queue is flushed, at the end of each event or when a
program calls `finish-output`, each note becomes a message written from the
record as it is *then*. A record added and then written to a hundred times
is sent once, with all its text.

That rule shaped the protocol itself. We first wrote insertions as
`(:insert parent index node)`, a position among the parent's children, and
found the case that breaks it: a record inserted and then removed before
the flush. The page never hears of it, but the positions of everything
inserted after it were counted with it in place. So an insertion now says
which sibling it goes in front of, worked out at the flush from the tree as
it is then, and the renderer cannot be told a position that is out of date.

The messages are printed with the world's own printer, under the standard
syntax so that nothing a program binds can change them:

```lisp
(defun flush-display ()
  (let ((messages (%take-messages)))
    (when messages
      (%display (with-standard-io-syntax
                  (let ((*print-readably* nil))
                    (prin1-to-string messages)))))
    messages))
```

A test shows the result. A block is added to a viewer, three cards are
counted in the default style and some dim text after them, and the flush
sends one message, describing the block and both runs:

```text
((:APPEND R1 (:BLOCK R2 NIL (:TEXT R3 NIL "3 cards") (:TEXT R4 (:INK :DIM) " (dim)"))))
```

(The test renames record numbers `R1`, `R2` in the order they appear, since
the real numbers depend on how many records the world made before.) Writing
more dim text afterwards sends only `(:SET R4 :TEXT " (dim) more")`, and a
flush with nothing changed sends nothing at all.

## The code that drew it

Phase 5 will offer *Show the code that drew this* on any output, and mock-up
03 of the environment's design shows why: you right-click the listener's
prompt and land in the method that prints it, change it, and the next
prompt is different. That needs every record to remember which function
made it, from the start; added later, it would mean going back through
every record-making path.

The record looks for it when it is made, on the stack: the innermost
function that is not part of the output machinery. Chapter 86 made every
frame keep its call, and `%frames` reads them all, but as a list, one cons
per frame, which is too much to pay per record. So there is a new primitive,
`%frame-callee`, which returns what one frame called without making
anything, and the record walks outwards with it:

```lisp
(defun %drawing-function ()
  (do ((i 1 (+ i 1)))
      ((> i 64) nil)
    (let ((callee (%frame-callee i)))
      (cond ((null callee) (return nil))
            ((%drawing-function-p callee) (return callee))))))
```

The machinery is anything named in the standard's package or in one of
Quokka's own system packages. `format`, `write-string` and the record
stream's methods are passed over, and the first function a person wrote,
or the environment's own `listener-prompt`, is the one kept. A method
counts by the name of its generic function; an anonymous function never
counts, since the definition around it is the one to show. The walk stops
at 64 frames, which keeps it bounded (rule 9) on a deep recursion.

## Viewers that draw again

Some viewers are drawn once and added to: the listener's transcript grows
at the bottom, and its old entries change only when something says so.
Others draw themselves from a model every time. The Rolodex of the
tutorials draws its list of cards, and after a card is added it draws the
list again; nobody wants to write the code that works out which line to
insert where. `docs/design.md` decided that after every command, every
visible viewer of this second kind redraws, which is CLIM's
`:display-time :command-loop`.

Redrawing everything would make every record new, and a new record is a new
element in the page: an editor that is redrawn loses its caret, and every
redraw sends the whole viewer. So the fresh drawing is compared with the old
one. Child by child, a fresh record of the same class in the same place as
an old one, or with the same `:key` when the program gave one, keeps the old
record's identity, and only its differences are sent. This is the technique
React made familiar in 2013 for its "virtual DOM", there over a tree of
page elements and here over output records, which is what the design asked
for when it said that redisplay compares records and not what is on the
screen. A record that moves to another place, matched by its key, is taken
away and added again, so its element in the page is made anew; only a
record that keeps its place keeps its element.

The test is a counter viewer whose `display-viewer` writes `count 0`, and
below it, once the count reaches 2, a line in the accent ink. After the
first key the page is sent one change, `(:SET R2 :TEXT "count 1")`; after
the second, the changed text and the new line, and nothing else.

## The command loop

The environment's top level is a loop in the main process. It waits for an
event, gives it to the viewer it is for, and then redraws and flushes. An
event arrives as text, an S-expression, and is read with the world's
reader under the standard syntax and with `*read-eval*` off, since the text
comes from outside; anything it cannot read is skipped, and a test sends it
`#.(error "no")`, an unfinished list and a number to make sure.

```lisp
(defun %handle-event (event)
  (restart-case
      (handler-bind ((error (lambda (condition)
                              (unless %listener-evaluating
                                (%report-command-error condition)
                                (invoke-restart 'abort)))))
        (%dispatch-event event)
        (%after-event))
    (abort ()
      :report "Return to the command loop."
      (flush-display)
      nil)))
```

An error in a viewer's method is written into the viewer, in the error ink,
and the loop goes on. The exception is an error in a form the listener is
running, which belongs to the listener and its debugger (chapter 98).

The same loop runs *inside* a form that wants input. When `read-line`, or
the debugger's prompt, finds nothing typed yet, the stream it reads from
runs `read-event-loop` until its line has been entered. Editing and the
other viewers go on working meanwhile, because the events are handled as
they always are. Genera's listener read this way, and so does every
desktop toolkit's modal dialog, which runs an event loop of its own until
it is answered. And because the only waiting the loop does is a host
request, a world waiting for the person is idle (chapter 94).

## What did it cost?

Two library files, `ui-records.lisp` and `ui-viewers.lisp`, and one
primitive. Writing them changed the protocol twice, as
described above: insertions by sibling, and a `(:focus id)` sent again when
a page is shown afresh, which the first version forgot, so that the caret
was nowhere when the environment started.
