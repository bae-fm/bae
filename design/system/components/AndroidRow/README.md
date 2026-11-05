# AndroidRow

One line of an `AndroidCard`: the title in 16, a subtitle in 14 under it, controls as children, and an icon on the right.

- Props: `title`, `subtitle`, `tone` (colours the subtitle and adds a dot: `success` for Synced), `icon` (`chevron` opens a page, `edit` opens a dialog to change it), `action`, `selected`.
- `action` draws the title in `accent`, for a row that is itself the action: "Invite".
- 72 tall with a subtitle, 56 without.

**In the code:** Android: `LabeledValueRow` (`ui/components/LabeledValueRow.kt`) for a row showing a value; the other rows have none yet.
