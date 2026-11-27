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
The `ring-*` drawings are the ring with its mark that Android's notices lead
with (`RING_DRAWING` in `components/src/30-both.js`), in their 20-unit box:
`ring-info`, `ring-success`, and `ring-alert` for warning and danger.
