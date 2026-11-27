# Tutorial: a Rolodex

> *An imagined tutorial.* Quokka's environment is designed but not built
> (`docs/design.md`, *Environment and UI*). This tutorial pretends it
> exists, to find out what writing a small application with lists and forms
> would feel like. Every name from `quokka-ui` and `quokka-stores` below is
> a proposal; the decisions it led to are in `docs/design.md`, *Platform,
> libraries and applications*, and the Tower of Hanoi tutorial
> (`docs/tutorials/tower-of-hanoi.md`) introduces viewers, drawing and
> commands more gently. The Common Lisp is real.

In this tutorial you will write a Rolodex, a card index of the people you
know. It has a list of cards with a search bar, an *Edit* and a *Delete*
button on every row, a *New card* button above the list, and an editor with
*Cancel* and *Save*. It takes about an hour. Along the way you will meet
private data that is saved as you work, commands that take an object as
their argument, forms whose fields check what you type, and a layout that
gives an application a desktop of its own.

## 1. A system and a package

Start as for any program, from the command line:

```
New System rolodex
```

and give the new package its definition:

```lisp
(defpackage :rolodex
  (:use :common-lisp :quokka-ui :quokka-stores)
  (:export #:card #:card-list))
```

`quokka-ui` is the platform: viewers, drawing, commands. `quokka-stores` is
a library built on it, for keeping data safe. The two never call each
other; your program is where they meet.

## 2. A card, and a store to keep them in

A card is a class with a slot for each thing you want to remember. Its
superclass, `private-object`, is a promise: a card never leaves this world.
If you export the Rolodex to give it to someone, it goes without your
cards, and Quokka refuses to export a card it finds anywhere else.

```lisp
(defclass card (private-object)
  ((name  :initarg :name  :accessor name)
   (phone :initarg :phone :initform "" :accessor phone)
   (email :initarg :email :initform "" :accessor email)
   (notes :initarg :notes :initform "" :accessor notes))
  (:metaclass stored-class))

(defmethod print-object ((card card) stream)
  (print-unreadable-object (card stream :type t)
    (princ (name card) stream)))
```

The cards themselves live in a **store**, a collection that knows how to
keep its contents safe:

```lisp
(defvar *cards* (make-store :private t :journalled t)
  "Every card in the Rolodex.")
```

`:private t` makes it a private store, which an export writes as an empty
one. `:journalled t` means that every change to it is written to the
world's journal the moment it happens, so a card you add survives a crash a
second later, long before the next snapshot. Try it in the listener:

```
rolodex 2 › (store-add *cards* (make-instance 'card :name "Ada Lovelace"
                                                   :phone "020 7946 0018"))
#<CARD Ada Lovelace>
rolodex 3 › (store-add *cards* (make-instance 'card :name "John McCarthy"
                                                   :phone "650 723 2273"))
#<CARD John McCarthy>
rolodex 4 › (store-items *cards*)
(#<CARD Ada Lovelace> #<CARD John McCarthy>)
```

## 3. Change cards in a transaction

A journal records whole changes, not every write to every slot: *add this
card*, *change that card's phone number and email*. So a card in a
journalled store is changed inside a **transaction**:

```
rolodex 5 › (with-transaction (*cards*)
              (setf (email (first (store-items *cards*))) "ada@example.org"))
"ada@example.org"
```

The `setf`s inside are ordinary. When the transaction ends, the store writes
all of them to the journal as one entry, which databases call *write-ahead
logging*: if the world stops at any moment, it opens again with all of the
change or none of it, never half. `(store-update *cards* card :email "...")`
is a shorthand for a transaction of one card's `setf`s, taking the same
keywords as `make-instance`.

The line `(:metaclass stored-class)` in `card` is what makes this work. It
gives cards a class that notices when one of their slots is written. Try
changing a card outside a transaction, as anyone would on their first day:

```
rolodex 6 › (setf (email (first (store-items *cards*))) "ada@lovelace.org")
The card Ada Lovelace is in the journalled store *CARDS*,
and was changed outside a transaction.
  1 · Make this change through the store
  2 · Change it anyway, unjournalled
  3 · Give up on this form
```

Choosing 1 journals the change as a transaction of its own. The store
library offers these restarts; the rule it enforces is its own, not the
platform's, and a program using a different kind of store would meet
different rules.

## 4. Finding cards

Searching is ordinary Lisp. `search` finds one sequence inside another, and
`:test #'char-equal` makes it ignore case:

```lisp
(defun matching-cards (text)
  "The cards whose name contains TEXT, in alphabetical order."
  (sort (remove-if-not (lambda (card)
                         (search text (name card) :test #'char-equal))
                       (store-items *cards*))
        #'string-lessp :key #'name))
```

`store-items` returns a fresh list every time, so `sort`, which may take
its argument apart, cannot damage the store. An empty search text is found
in every name, so `(matching-cards "")` is every card.

## 5. The list

The list is a viewer. Its only state of its own is the search text:

```lisp
(define-viewer card-list ()
  ((search-text :initform "" :accessor search-text))
  (:title "Rolodex")
  (:minimum-size 360 240))

(defmethod display-viewer ((v card-list) stream)
  (text-field stream (search-text v) :placeholder "Search names")
  (let ((cards (matching-cards (search-text v))))
    (if (null cards)
        (format stream "~&No card matches ~S." (search-text v))
        (formatting-table (stream)
          (dolist (card cards)
            (formatting-row (stream)
              (formatting-cell (stream) (present card 'card :stream stream))
              (formatting-cell (stream) (princ (phone card) stream))
              (formatting-cell (stream)
                (command-button stream 'edit-card card)
                (command-button stream 'delete-card card))))))))
```

Three things in it are new.

1. `text-field` draws an input field bound to a **place**, anything `setf`
   can write to, here the viewer's `search-text`. Each time you type, the
   platform runs one built-in command, *edit field*, which does the `setf`.
   A command runs, the viewers redraw, and the list shrinks as you type. You
   wrote no command for it. The field keeps its cursor and selection across
   redraws, because it is the same output record each time.
2. `formatting-table` lays out rows and cells and works out the column
   widths, so the phone numbers line up without your measuring anything.
3. `command-button` here takes an argument, the card on that row. Edit and
   Delete belong to a row, not to the tile, so they are placed inside the
   content; that is what `command-button` is for. Commands that belong to
   the whole tile go in its command bar.

`present` draws the card's name as a presentation of the card, so you can
right-click any name to inspect it.

## 6. The commands of the list

```lisp
(define-viewer-command (new-card :viewer card-list
                                 :label "New card" :bar t :keys (#\n)
                                 :when (not (editing-p)))
    (v)
  (open-editor nil))

(define-viewer-command (edit-card :viewer card-list :label "Edit"
                                  :when (not (editing-p)))
    (v (card 'card))
  (open-editor card))

(define-viewer-command (delete-card :viewer card-list :label "Delete"
                                    :confirm (format nil "Delete ~A?" (name card)))
    (v (card 'card))
  (store-remove *cards* card))
```

`new-card` has `:bar t`, so it sits in the tile's command bar above the
list, beside the zoom and close decorations, and the key `n` runs it while
the list has the focus.

`edit-card` and `delete-card` take a second argument of presentation type
`card`. That is what lets the row buttons pass their card, and it gives you
more for nothing: right-click any card anywhere on the screen and *Edit*
and *Delete* are in its menu, and typing `Delete Card` in the command line
waits for you to click the card you mean.

`:when (not (editing-p))` is this Rolodex's rule for unsaved work: while a
card is open in the editor, New and Edit do not apply, so a draft can never
be pushed aside. `editing-p` comes in the next section. Quokka has no rule of
its own about unsaved work; another program might ask, or keep drafts, and
the platform leaves that to it.

`:confirm` asks before the command runs, with the text it is given. Deleting
is the one thing in the Rolodex you cannot take back by pressing Cancel.

## 7. The editor

The editor is a second viewer. It never changes a card while you type.
It edits a **draft**, a property list of the card's fields, and only *Save*
puts the draft into the store, in one journalled change. *Cancel* throws
the draft away, and the card is as it was.

```lisp
(define-viewer card-editor ()
  ((card :initarg :card :initform nil :reader edited-card)
   (draft :accessor draft))
  (:minimum-size 300 220))

(defmethod initialize-instance :after ((e card-editor) &key)
  (let ((card (edited-card e)))
    (setf (draft e)
          (if card
              (list :name (name card) :phone (phone card)
                    :email (email card) :notes (notes card))
              (list :name "" :phone "" :email "" :notes "")))))

(defmethod viewer-title ((e card-editor))
  (if (edited-card e)
      (format nil "Edit ~A" (name (edited-card e)))
      "New card"))

(defmethod display-viewer ((e card-editor) stream)
  (accepting-values (stream)
    (text-field stream (getf (draft e) :name)  :label "Name"  :type 'name-text
                :check #'name-free-p)
    (text-field stream (getf (draft e) :phone) :label "Phone" :type 'phone-number)
    (text-field stream (getf (draft e) :email) :label "Email" :type 'email-address)
    (text-field stream (getf (draft e) :notes) :label "Notes" :type 'paragraph)))

(defun editing-p ()
  "True while the editor slot holds a card being edited."
  (viewer-in-slot :detail))
```

The fields are the same `text-field` as the search bar, bound to places in
the draft; `getf` is a place, so `setf` can write into a property list.
`accepting-values` only lays them out as a form, one labelled field to a
row.

What a field accepts is its presentation type. A presentation type turns
the text you type into a value, and when the text cannot be one, it signals
a condition saying why:

```lisp
(define-presentation-type name-text () :inherit-from 'string)
(define-presentation-method parse-input ((type name-text) text)
  (let ((name (string-trim " " text)))
    (if (plusp (length name))
        name
        (error 'input-not-of-required-type :string text :type type
               :format-control "A card needs a name."))))

(define-presentation-type email-address () :inherit-from 'string)
(define-presentation-method parse-input ((type email-address) text)
  (if (or (string= text "") (find #\@ text))
      text
      (error 'input-not-of-required-type :string text :type type
             :format-control "An email address has an @ in it.")))

(define-presentation-type phone-number () :inherit-from 'string)
(define-presentation-method parse-input ((type phone-number) text)
  (if (every (lambda (c) (or (digit-char-p c) (find c " +-()"))) text)
      text
      (error 'input-not-of-required-type :string text :type type
             :format-control "A phone number is digits, spaces and + - ( ).")))
```

The field catches the condition and shows its report under itself as you
type, and its border turns red. The types are reusable: an `email-address`
field in any other program checks itself the same way. Parsing and checking
are one step on purpose: a `date` field would return a date, not a string
that looks like one.

A rule that belongs to this program and not to the type goes in `:check`,
a function of the parsed value. The Rolodex wants names to be unique, which
only it can know:

```lisp
(defun name-free-p (name)
  (or (notany (lambda (card) (string-equal (name card) name))
              (store-items *cards*))
      "There is already a card with that name."))
```

(Editing Ada and leaving her name alone must not count as a clash; the
full version skips the card being edited.)

Save and Cancel are the editor's commands, in its own command bar:

```lisp
(define-viewer-command (save-card :viewer card-editor
                                  :label "Save" :bar t :keys (#\Return)
                                  :when (viewer-fields-valid-p e))
    (e)
  (let ((card (edited-card e)))
    (if card
        (apply #'store-update *cards* card (draft e))
        (store-add *cards* (apply #'make-instance 'card (draft e)))))
  (close-viewer e))

(define-viewer-command (cancel-edit :viewer card-editor
                                    :label "Cancel" :bar t :keys (#\Escape))
    (e)
  (close-viewer e))
```

`viewer-fields-valid-p` is a plain function the platform offers: true when
no field in the viewer is showing a complaint. Using it in Save's `:when` is
this program's choice; hovering over Save says which field is wrong. The
draft's keys are the card's initargs, which is why one `apply` serves both
a new card and a changed one, and `store-update` makes the change one
transaction.

## 8. A desktop of its own

Where does the editor open? Not on top of the list, and not in some tile
Quokka picks: the Rolodex gets a layout of its own, with a slot for the list
and a slot for the editor.

```lisp
(define-layout rolodex-layout ()
  (:side-by-side (:slot :main :weight 3)
                 (:slot :detail :weight 2)))

(defun open-editor (card)
  (open-viewer 'card-editor :card card :slot :detail))
```

`:slot :detail` puts the editor in the slot of that name on the current
desktop. Now tell the system which viewer it starts with and which layout
it uses. The system's own definition is in the browser, at the top of the
systems column; add two lines to it:

```lisp
(define-system :rolodex
  (:packages :rolodex)
  (:main-view rolodex:card-list)
  (:layout rolodex:rolodex-layout))
```

## 9. Using it

```
Start Application rolodex
```

Quokka opens a new desktop called *rolodex*, with the list on the left and
an empty slot on the right that says *Nothing open*. Type `mc` in the search
bar and only John McCarthy is left. Click *Edit* on his row and the editor
opens on the right. While it is open, New card and the other rows' Edit
buttons do not apply. Type a phone number with a letter in it, watch the
field complain and Save stop applying, fix it, and press Return. Click *New
card* above the list, or press `n`, and the editor opens empty.

The desktop is one of your desktops now. Switch back to the one with the
listener, and the Rolodex is still there, with its search text and its open
editor, when you come back. Close the tab and open the world tomorrow, and
it is still there too, with every card you added.

## 10. Changing it while it runs

Add a birthday. Add a slot to `card`:

```lisp
(birthday :initarg :birthday :initform "" :accessor birthday)
```

press Cmd-Enter, and add one line to the editor's form and one to its
draft. Every card you already have is the same object as before, with a new
slot; CLOS updates instances when their class changes. The list and the
open editor redraw, and no card was lost.

## 11. Keeping it, and giving it away

Commit when you are happy, with a message such as *Rolodex, first version*.
To give the Rolodex to someone, export it:

```
Export System rolodex As Web Page :locked
```

They get the list, the search bar, the editor and the buttons, and an empty
Rolodex, because `*cards*` is a private store. Your cards stay with you.

## Try next

- *Undo* instead of asking before Delete. A journalled store can undo its
  own last change, because its journal holds what each change overwrote:
  `(store-undo *cards*)`. The store does not know about commands, so the
  Rolodex wires it up itself, with an *Undo* command in the list's bar
  (`:when (store-can-undo-p *cards*)`) and Delete without its `:confirm`.
- A package `rolodex.import` in the same system that reads vCard files,
  with a command *Import Cards* that takes a file.
- Highlight a card whose birthday is today, with an ink from the theme.
- Give a plug-in the store's reading half only, and show that it cannot
  change a card: a first exercise in object capabilities.
