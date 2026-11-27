# Symbols

The icons that the design system's components and the website draw, one file
for each drawing, which both read. A file named for a Material Symbol holds
that symbol in Material Symbols Rounded at weight 400, the set Android draws:
Google's drawing in its 960-unit box. `build-bundle.py` puts every file into
the components' bundle by its name (`SYMBOL_DRAWING` in
`components/src/00-base.js`). A file holds only an svg with a `viewBox` and
paths, each with its `d` and at most a `fill-rule`, which is all the components
draw. The website draws a symbol as a mask in the colour of the text around it,
and draws a status glyph (`error`, `check_circle`, `info`) in its tone's
colour, as `StatusGlyph` draws Android's.
`ring` is the ring that Android's notices lead with, and each `ring-mark-*` is
a mark that sits inside it, all in the ring's 20-unit box: `ring-mark-info`,
`ring-mark-success`, and `ring-mark-bang` for warning and danger. The
components draw a tone's mark over the ring in one svg (`RING_DRAWING` in
`components/src/30-both.js`), and the website draws the two as two masks, one
over the other.
