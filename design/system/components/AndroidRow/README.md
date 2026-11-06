# AndroidRow

`icon`: `chevron` when the row opens a page, `edit` when it opens a dialog to change the value. `action` for a row that is itself the action, with `leadingIcon` `add` when it adds to the list: "Add device", "Invite", last in the card. `nested` for a setting that belongs to the switch row above it, with no divider between them: "Countdown" under "Pause between sides and discs".

**In the code:** Android: `LabeledValueRow` (`ui/components/LabeledValueRow.kt`) for a row showing a value; the other rows have none yet.
