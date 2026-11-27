# Drawing

> *Where this fits.* The renderer of chapter 97 draws text, with the
> browser laying it out. The design calls the renderer a hybrid: text in the
> page's document, and pictures as SVG or on a canvas. This chapter adds the
> pictures, as output records like any other, so that the redisplay of
> chapter 96 keeps a shape and moves it, and phase 5 can make a shape a
> presentation you can click. It came after phase 4's gate, which did not
> need it, and it is the last step of the phase.

*Status: done, issue #143, step I of phase 4.*

## A picture is records too

The Tower of Hanoi tutorial draws its pegs and disks like this:

```lisp
(with-drawing (stream :width 300 :height 180 :scale :fit)
  (draw-rectangle* 10 170 290 180 :ink +structure+)
  ...)
```

`with-drawing` makes a **drawing record**, a box of its own measured in
logical pixels, and each `draw-` function adds a **shape record** to it.
A shape is an output record with an identity, a parent, the function that
drew it and properties: `(:type :ellipse :cx 25 :cy 25 :rx 20 :ry 20 :ink
:accent :filled t)`. Nothing about it is new to the protocol; the renderer
learns three kinds, `:drawing`, `:group` and `:shape`, and turns them into
SVG's `<svg>`, `<g>` and one element per shape.

We chose SVG over the canvas for the first renderer of pictures because an
SVG shape is an element of its own. Redisplay can keep it and change only
its place; a click can find it, which phase 5 needs for presentations; and
the browser scales it without blurring. A canvas is a grid of pixels that a
program paints, faster for a picture that changes every frame and with
nothing in it to keep. A game that redraws sixty times a second will want
it, and it can come later as another way of drawing the same `:drawing`
kind.

The drawing functions take no stream. They draw into the picture
`with-drawing` is making, which is in a special variable, `*drawing*`, so a
function that draws part of a scene can be called from inside one without
being handed anything, as the platform game of the tutorials does. CLIM's
functions take a *medium* as their first argument; the tutorials were
written without one, and we kept their shape.

## Coordinates and transformations

A picture's numbers are any real numbers, ratios included, since a program
that places disks by dividing gets ratios. The page reads integers and
decimals, so a ratio is sent as the double it stands for. The chapter's
first test drew a rectangle `1/2` wide; its first version wrote `(float x
1d0)` to make the double, and the world would not build, because a float
written in a library file is read at build time by the bootstrap's reader in
Rust, which knows no floats and read `1D0` as a symbol (chapter 48 warned
about this, and we had forgotten). `(float x)` makes the same double.

`with-translation` and `with-scaling` are CLIM's affine transformations, a
group of shapes under the matrix `(a b c d e f)` that SVG's `transform` and
the canvas's `setTransform` both take, so the renderer passes it on as it
is. A box drawn with `:scale :fit` keeps its proportions as it grows with
its tile, which is SVG's `viewBox`; `:stretch` fills the tile; and no scale
shows the picture at its size.

## Redrawn shapes keep their identity

The second test is a viewer that draws a circle at a position in its model,
and moves it ten pixels for every key. After a key, the page is sent one
message, the circle's new centre, and no new shape: reconciliation found a
shape of the same class in the same place in the fresh drawing, and kept
the old one. That is what will let Hanoi's disks move without the picture
flickering, and what will let a disk be the same presentation before and
after it moves.

## What did it cost?

One library file, `ui-drawing.lisp`, the SVG half of the renderer and its
colours in the theme, and no Rust. A generic function,
`stream-record-stream`, lets a stream that is not a record stream say where
its pictures go, so the drawing code does not have to know the listener.
