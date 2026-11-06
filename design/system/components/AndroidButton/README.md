# AndroidButton

`filled` for the action a screen is for, `outlined` beside it, `text` in dialogs and rows. `large` for the welcome screen's actions. Words are a verb, sentence case, no ellipsis.

A `text` button on its own line lines its words up with the text above it: `alignTo: 'text'` under plain text, flush with it; `alignTo: 'field'` under a field, with the field's text, 16dp in.

**In the code:** Android: `PrimaryButton` (`ui/components/PrimaryButton.kt`) for filled; Material's `OutlinedButton` and `TextButton`.
