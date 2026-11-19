# MacButton

A macOS push button: `kind="primary"` for the action a screen is for, secondary otherwise.

- `off` dims a button that can't act now.
- Primary is the deep accent (`accent-selection`) with white text, as the Settings sidebar's chosen pane: the bright accent fill glares in a button. In an alert, the default button is primary; a destructive one is `destructive`.
- `kind="borderless"` draws the words in the accent with no fill: a notice's Details, a text button in a row.
- `neutral` draws a borderless button's words in primary text: a step's Cancel beside its default button, as Create and Join draw it.
- `muted` draws the label in secondary text, for buttons under quiet words: a failed view's centred notice.
- `tinted` is a secondary button on a filled notice: its fill is the notice's text at 12%, so it reads on every tone (`MacNotice` passes it to its action). `size` is small, regular (unset) or large, the system's control sizes; a cast row's Disconnect is small. `size="alert"` is the button of a `MacAlert`: full width, 28pt high.

`label` names a button whose words don't say what it does: the import draft's ⋯ is named Metadata.

`menu` for a button whose click opens a menu, such as Welcome's Add library…: it looks like any button, and macOS reads it as a pop-up button.

**In the code:** macOS: `Button`; with `menu`, a `Menu` with `.menuStyle(.button)` and `.menuIndicator(.hidden)`; primary with BaeKit's `PrimaryButtonStyle`, borderless with BaeKit's `.accentBorderless` style, `neutral` in primary text, as `CreateNameView` and `JoinNameView` draw their Cancel, destructive with `role: .destructive`.
