# AndroidRow

`icon`: `chevron` when the row opens a page, `edit` when it opens a dialog to change the value. `action` for a row that is itself the action, with `leadingIcon` `add` when it adds to the list: "Add device", "Invite", last in the card. `value` puts the label's value at the row's far end, in the secondary colour, as `MacRow`'s does: a member's details, a restore form's summary. `nested` for a setting that belongs to the switch row above it: joined to the switch's row in one piece of the card, with no gap, and set in from the card's edge so it reads as under the switch: "Resume automatically" under "Pause between sides and discs".

**In the code:** Android: `AndroidRow` (`ui/components/AndroidRow.kt`); `LabeledValueRow` (`ui/components/LabeledValueRow.kt`) draws `value` today, in dialogs; it becomes `AndroidRow` with `value`.
