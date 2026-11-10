# MacRow

`chevron` when the row opens something. `action` for a row that is itself the action, at the end of the list it adds to: "Invite…". `selected` for the chosen row in a choice list (a folder in the set-up sheet): solid accent with white text, as the Settings sidebar's chosen pane. A list you browse, such as the import list, marks its selection with the tint instead. `nested` for a setting that belongs to the switch row above it: no gap and no divider between them, on the same left edge, so the two read as one setting: "Resume automatically" under "Pause between sides and discs". `off` dims it while that switch is off.

**In the code:** macOS: `LabeledContent` in a grouped `Form`.
