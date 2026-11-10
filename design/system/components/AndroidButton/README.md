# AndroidButton

`filled` for the action a screen is for, `outlined` beside a filled one, `tonal` for an action that stands alone without being what the screen is for (a centred notice's Retry or Finish resetting, Check now, New playlist), `text` in dialogs and rows. An outline only ever marks the lesser of two. `large` for the welcome screen's actions. Beside a `destructive` text button, the other one (Cancel, Keep) is `neutral`, in the screen's text colour, so the accent and the danger colour never sit side by side. Words are a verb, sentence case, no ellipsis.

A `text` button on its own line lines its words up with the text above it: `alignTo: 'text'` under plain text, flush with it; `alignTo: 'field'` under a field, with the field's text, 16dp in.

**In the code:** Android: `PrimaryButton` (`ui/components/PrimaryButton.kt`) for filled; Material's `FilledTonalButton`, `OutlinedButton` and `TextButton`.
