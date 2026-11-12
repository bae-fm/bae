# MacRow

`chevron` when the row opens something. `action` for a row that is itself the action, such as "Remove from library…". A button that adds to the list ("Invite…", "Add device…", "Add format…") isn't a row: it's a `MacSheetFooter` under the box, on the right, as System Settings puts "Add Account…". `selected` for the chosen row in a choice list (a folder in the set-up sheet): solid accent with white text, as the Settings sidebar's chosen pane. A list you browse, such as the import list, marks its selection with the tint instead. `nested` for a setting that belongs to the switch row above it: no gap and no divider between them, on the same left edge, so the two read as one setting: "Resume automatically" under "Pause between sides and discs". `off` dims it while that switch is off. On any other row, `off` dims a row that can't be used for now, such as the other libraries while one is being removed.

`copy` adds text after the note as a `MacCopyText`, with a copy button on hover: “Apps connect at” then an address. Given more than one, the note ends in a colon and each is a bullet under it, with words after it that aren't copied: “Apps connect at:” / “• http://192.168.1.20:4533 (Wi-Fi)” / “• http://100.64.0.3:4533 (Tailscale)”.

A row the person clicks reads as one button named by its words: one with `chevron`, `action` or `destructive` is, and `button` marks the rest, such as a Welcome library. A row that ends in controls of its own (a ⋯ menu) gives the button to its label alone.

**In the code:** macOS: `LabeledContent` in a grouped `Form`; a row the person clicks is a `Button` or `NavigationLink` whose label is the row.
