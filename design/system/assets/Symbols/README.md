# Symbols

The icons the website draws: Material Symbols Rounded at weight 400, the set
Android draws, each Google's drawing in its 960-unit box. Where the components'
Android icons hold the same symbol (`SYMBOL_PATH` in `components/src/00-base.js`),
the drawing here is the same one. The website draws each as a mask in the
colour of the text around it, and draws a status glyph (`error`,
`check_circle`, `info`) in its tone's colour, as `StatusGlyph` draws Android's.
The `ring-*` drawings are the ring with its mark that Android's notices lead
with (`RING_DRAWING` in `components/src/30-both.js`), in their 20-unit box:
`ring-info`, `ring-success`, and `ring-alert` for warning and danger.
