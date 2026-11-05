# Button

One primary button per screen: the action the screen is for, on the right of the footer. Everything else is secondary.

- Props: `kind` (`primary`, `secondary`, `icon`), `label` or children for the words, `off`.
- Primary is `accent-fill` with `on-fill` text; secondary is `tile` with `text-primary`. An icon button is a glyph in `text-secondary`, no fill; its `label` is read aloud, never shown. With no children it shows the copy glyph.
- Words are a verb, sentence case: "Set up", "Cancel", "New folder…". An ellipsis means the button opens more before acting.
- Off buttons sit at `opacity-off`. Don't hide a button that can come back; turn it off.
- Height `size-control` (22px), 10px across, radius `radius-control`.
