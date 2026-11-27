# Symbols

The icons the website draws: Material Symbols Rounded at weight 400, the set
Android draws, each Google's drawing in its 960-unit box. Where the components'
Android icons hold the same symbol (`SYMBOL_PATH` in `components/src/00-base.js`),
the drawing here is the same one. The website draws each as a mask in the
colour of the text around it, and draws a status glyph (`error`,
`check_circle`, `info`) in its tone's colour, as `StatusGlyph` draws Android's.
