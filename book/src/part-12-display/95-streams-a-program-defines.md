# Streams a program defines

> *Where this fits.* Since chapter 38 Quokka has had streams: a string to
> read from, a string to write to, and the console. They were enough for
> the reader, the printer and the debugger, and they were all the library's
> own. The display protocol of chapter 94 needs a stream that writes output
> records into a viewer, and that is something no fixed list of kinds can
> include. This chapter makes streams a class a program can define, using
> the protocol every Common Lisp provides for it.

*Status: done, issue #136, step B of phase 4.*

## Three structures and a `cond`

Every stream function in the library began the same way. `write-char`
asked whether its stream was the console or a string output stream;
`read-char` assumed a string input stream, of which the console's input is
one, refilling itself from the host at its end (chapter 88). The three kinds
are structures, and every function knows all three.

That is a closed world. The listener's stream has to turn what `format`
writes into text records in a viewer (chapter 96), and later a presentation
stream has to remember which object each piece of text shows. Adding each
as a fourth and fifth structure, with a branch in every function, would put
the toolkit's knowledge into the printer. What is wanted is for `write-char`
to ask the stream itself what to do.

## The proposal nobody adopted and everybody implemented

Common Lisp's standard has streams but no way to define one. In 1989 David
Gray, of Texas Instruments, proposed one to the standards committee: a
stream is an instance of a class, and each standard function calls a
generic function on it, so `write-char` calls `stream-write-char` and
`read-char` calls `stream-read-char`. The committee did not adopt it, CLOS
being new and the standard nearly finished, and every implementation since
has provided it anyway, SBCL in its package `SB-GRAY`. Libraries that need
a stream of their own, such as a stream that counts lines or one that
writes into a buffer, are written against it, usually through the small
portability library `trivial-gray-streams`.

The proposal is economical about what a class has to define. A character
output stream needs two methods, `stream-write-char` and
`stream-line-column` (which may answer nil, "I don't know"), and has
defaults for everything else, built on those two:

```lisp
(defgeneric stream-write-string (stream string &optional start end))
(defmethod stream-write-string ((stream fundamental-character-output-stream) string
                                &optional (start 0) end)
  (let ((end (or end (length string))))
    (do ((i start (+ i 1)))
        ((>= i end) string)
      (stream-write-char stream (char string i)))))
```

A class that can do better overrides the default: the listener's stream
will take a whole string at once rather than a character at a time.
Input works the same way, with `stream-read-char` and `stream-unread-char`
required and `stream-read-line`, `stream-peek-char` and `stream-listen`
built on them.

We put Quokka's in a package of its own, `QUOKKA-GRAY`, with SBCL's names,
so that a test can define a stream class on both and compare. The class
`fundamental-stream` has `stream` among its superclasses, as in SBCL, which
needed one change to the class system: `defclass` had refused every
built-in class as a superclass of a standard one, and now accepts `stream`.

## Keeping the fast paths

The reader and printer are the hottest code in the library. Chapter 58
found that every step spent per character shows up in ansi-test's step
budget, and took the reader's character loop down to a structure access
and a comparison. So the structure streams keep their paths, and the
generic functions are where the paths end:

```lisp
(defun %next-char (stream)
  (if (%string-input-stream-p stream)
      (let ((at (%string-input-stream-position stream)))
        (if (< at (%string-input-stream-end stream))
            (progn (setf (%string-input-stream-position stream) (+ at 1))
                   (aref (%string-input-stream-string stream) at))
            (if (%refill-console stream) (%next-char stream) nil)))
      (let ((char (stream-read-char stream)))
        (if (eq char :eof) nil char))))
```

The test at the top costs a few steps a character, which is what checking
the structure's type already cost inside the accessor. A Gray stream goes
straight to the generic function. The same shape is in `read-char`,
`unread-char`, `read-line`, `write-char`, `terpri`, `fresh-line` and the
rest; `streamp` is true of both kinds.

Six standard functions are generic functions in SBCL, so that a stream
class can answer them: `close`, `open-stream-p`, `input-stream-p`,
`output-stream-p`, `interactive-stream-p` and `stream-element-type`. They
are generic in Quokka too, with a method on `t` that answers for the
structure streams. That turned up an old assumption. `close` takes a
keyword argument, `:abort`, and a generic function with keyword arguments
works out the keywords' names when it is defined, by calling `intern` into
the keyword package. `intern` is defined in `packages.lisp`, the last file
of the library, and `gray-streams.lisp` comes before the reader, long
before it. No generic function defined while the library loads had taken
keywords until now. The machine has always had a primitive that makes a
keyword without a package system, for the reader in Rust, and the
generic-function code now uses that.

## Testing it against SBCL

The test file defines three streams in a package that uses `SB-GRAY` under
SBCL and `QUOKKA-GRAY` under Quokka: an output stream that upcases what it
is given and counts its column, one that keeps whole strings, and an input
stream over some lines of text. Then it uses them through the standard
functions, `format`, `print`, `fresh-line`, `read`, `read-line`,
`read-sequence`, `with-open-stream`, and compares every answer with
SBCL's. One of them:

```lisp
(let ((s (lines "(+ 1 2) foo \"bar\" #(1 2)")))
  (list (read s) (read s) (read s) (read s) (read s nil :eof)))
```

The whole reader runs over a stream it has never heard of, and gives
`((+ 1 2) FOO "bar" #(1 2) :EOF)`, as SBCL does.

## What did it cost?

One library file, `gray-streams.lisp`, and a branch at the end of each
stream function; no Rust. Gray streams make a program's
output go anywhere a class can send it. The next chapter makes a class
that sends it into a viewer.
