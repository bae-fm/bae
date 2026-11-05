# MacRow

One line of a `MacBox`: the label, then on the right a grey `value`, any controls passed as children, and a chevron when the row opens something.

- Props: `label`, `strong`, `note` (a second line in `detail`), `value`, `tone` (colours the value and adds a dot: `success` for Synced), `chevron`, `selected`, `action`.
- `action` draws the label in `accent`, for a row that is itself the action: "Add device…".
- A selected row in a choice list takes `accent-tint`.
