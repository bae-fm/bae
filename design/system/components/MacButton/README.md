# MacButton

One primary button per screen: the action the screen is for, on the right of the footer. Everything else is secondary.

- Words are a verb, sentence case. An ellipsis means the button opens more before acting.
- Don't hide a button that can come back; turn it `off`.
- A Remove at the end of a row is `borderless` and `destructive`.
- Primary is the deep accent (`accent-selection`) with white text, as the Settings sidebar's chosen pane: the bright accent fill glares in a button. In an alert, the default button is primary; a destructive one is `destructive`.
- `muted` draws the label in secondary text, for buttons under quiet words: a failed view's centred notice.
- `tinted` is a secondary button on a filled notice: its fill is the notice's text at 12%, so it reads on every tone (`MacNotice` passes it to its action). `size="alert"` is the button of a `MacAlert`: full width, 28pt high.

`menu` for a button whose click opens a menu, such as Welcome's Add library…: it looks like any button, and macOS reads it as a pop-up button.

**In the code:** macOS: `Button`; with `menu`, a `Menu` with `.menuStyle(.button)` and `.menuIndicator(.hidden)`; primary with BaeKit's `PrimaryButtonStyle`, borderless with `.buttonStyle(.borderless)`, destructive with `role: .destructive`.
