# MacButton

One primary button per screen: the action the screen is for, on the right of the footer. Everything else is secondary.

- Words are a verb, sentence case. An ellipsis means the button opens more before acting.
- Don't hide a button that can come back; turn it `off`.
- A Remove at the end of a row is `borderless` and `destructive`.
- `muted` draws the label in secondary text, for buttons under quiet words: a failed view's centred notice.

**In the code:** macOS: `Button`; primary with BaeKit's `PrimaryButtonStyle`, borderless with `.buttonStyle(.borderless)`, destructive with `role: .destructive`.
