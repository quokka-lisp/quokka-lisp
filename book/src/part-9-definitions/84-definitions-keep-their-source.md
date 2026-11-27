# Definitions keep their source

> *Where this fits.* Everything so far has treated a definition as
> something that happens: `defun` compiles a function, installs it, and is
> gone. That was enough while every line of Quokka lived in a file. Phase 3
> is building towards a debugger in which a person fixes a function from
> inside an error, and for that the world has to know what the function's
> definition said. This chapter makes a world remember its definitions: the
> form, the text it was written as, where it came from, and what it
> replaced. It is the first layer of the environment's *database of
> definitions* (#126), which later chapters build the browser, the inspector
> and the history on.

*Status: done, issue #55. Phase 3, step C, sub-step 1. Cross-reference and
recompilation are the next sub-steps.*

## What a world forgot

Type a function into a Quokka world:

```lisp
(defun sq (x) (* x x))
```

The world compiles it, installs the result as `sq`'s function, and prints
`SQ`. Until this step, that was all that was left of it: bytecode and a
table of constants. Nothing could show you the definition again, and
`function-lambda-expression`, Common Lisp's way of asking a function what
it was made from, always answered that it did not know. The standard allows
that answer, and SBCL gives it too for compiled code, because SBCL expects
its source to be in files, which it can find again from a recorded file
name and position.

A Quokka world is meant to be different. `docs/design.md` decided, on #126,
that a running world is the truth and files are where it is written: you
work inside the world, and the world writes your definitions out to files
when you commit them. For that the world has to keep them, and keep them as
text, because a file written back must say exactly what you wrote, comments
and all, and the reader throws comments away.

## A definition is an object

A definition is now an instance of a class, `definition`, with its kind
and name, its form, its text, the file and position it came from, the
package it was read in, the object it made, and the definitions it
replaced:

```lisp
(defun sq (x) (* x x))
(find-definition 'sq)                     ; #<DEFINITION :FUNCTION SQ>
(definition-text (find-definition 'sq))   ; "(defun sq (x) (* x x))"
```

These names are Quokka's, not Common Lisp's, and live in package `QUOKKA`
for now, so from `CL-USER` they are written `quokka::find-definition`; the
environment will give them a package of its own.

The world keeps them in one table, keyed by kind and name, since a symbol
can name a function, a variable and a class at once. A method's name is
its generic function's name, its qualifiers and its specializers, so the
two methods of one generic function are two definitions.

We considered keeping the source on the function object itself, as a new
slot. That would have meant Rust, a change to the image format and to the
verifier, and it would have covered only functions, where a world defines
classes, variables and macros too. A function already carries the name it
was compiled under, for the disassembler, so the one piece of Rust is a
primitive that lets Lisp read it, and a function finds its definition by
that name:

```lisp
(function-lambda-expression #'sq)          ; (LAMBDA (X) (* X X)), T, SQ
```

`function-lambda-expression` answers only if the function is still the one
the definition made; a function installed some other way has no definition
to speak for it.

## Where definitions are recorded

A definition is recorded where a top-level form is evaluated, which is
three places: `load`, reading a file form by form; the host's
read-eval-print loop, reading what a person typed; and `eval`, given a
defining form by a program. The first two have the text the form was read
from. `eval` has only the form, so a definition made by `eval` keeps its
form and no text.

Which forms define what is a table, from `defun` (a function) and
`defmacro` (a macro) to `defpackage` (a package), eighteen operators in all.
A program's own `def...` macro is not in it: there is no telling from a name
what, if anything, a macro defines.

The world's own definitions, the library and the compiler, are not
recorded in this step. The cold build loads them with the Rust reader, not
with `load`, and the warm rebuild turns recording off. Recording them would
put the 850 KB of the library's and the compiler's source into every image,
and as strings of a word per character, since a string cut from a file is a
writable one, that is several times the image's own 1.9 MB, before anything
uses it. The environment of phase 4 reads the system's own files in when it
needs to browse them.

## Which comments belong to a definition

Cutting a definition's text out of a file sounds simple: take the
characters from the end of the previous form to the end of this one. Our
first version did that, and a file's opening comment came out as part of
its first definition. #126 had decided the rule already: the comments
directly above a definition, with no blank line between, are its own, and
a file's opening comments belong to its package. So the text now starts
after the last blank line above the form.

The other end has the opposite problem. A comment at the end of a
definition's last line, as in

```lisp
(defvar *counter* 0)   ; how many we have seen
```

belongs to that definition, and the reader stops reading at the closing
parenthesis, before it. The text now runs on to the end of a comment that
ends the line, and the next definition's text starts after it. Two forms on
one line, which a person typing at the prompt often writes, broke the first
version of that rule, and a test caught it: the second form's text started
at the next line, after its own end.

## Versions

Evaluating `defun sq` again replaces the definition, and the new one keeps
the old one as a version, newest first:

```lisp
(defun sq (x) (* x x x))
(length (definition-versions (find-definition 'sq)))   ; 1
```

#126 gives definitions a history with commits and a way back. This step
keeps only the list; phase 4 adds the rest.

## What did it cost?

The Rust is one primitive, `%function-name`, which reads the name a
compiled function was assembled under, through a closure to the function
it holds. The rest is Lisp: a new library file, `definitions.lisp`, and a
few lines in `eval`, `load` and the host loop. The test is in Rust, since
SBCL answers nil to `function-lambda-expression` and so cannot be the
oracle.

## What is still missing?

The world knows its definitions, but not how they depend on each other. A
macro is expanded when the functions that use it are compiled, so
redefining it leaves them with the old expansion, and the world cannot yet
tell which they are. The next sub-step has the compiler record what each
definition calls and which macros it expands; the one after recompiles a
definition from its kept form.
