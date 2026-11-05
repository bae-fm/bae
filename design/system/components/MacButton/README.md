# MacButton

One primary button per screen: the action the screen is for, on the right of the footer. Everything else is secondary.

- Props: `kind` (`primary`, `secondary`, `icon`), `label` or children for the words, `destructive`, `off`.
- Primary is `accent-fill` with `on-fill` text; secondary is `tile`. `destructive` sets the words in `danger` ("Remove…"). An icon button is a glyph in `text-secondary`, no fill; its `label` is read aloud, never shown.
- Words are a verb, sentence case. An ellipsis means the button opens more before acting.
- Off buttons sit at `opacity-off`. Don't hide a button that can come back; turn it off.

**In the code:** macOS: `Button`; primary with BaeKit's `PrimaryButtonStyle`, destructive with `role: .destructive`.
