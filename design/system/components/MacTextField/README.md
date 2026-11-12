# MacTextField

A placeholder never repeats the label and never ends in an ellipsis. With a visible label, leave the empty field blank, unless the placeholder says what the field is for that the label can't: “Appears on your profile, ratings and playlists” under the label “Your name”. It has to fit the box on one line.

`code` is the field for a restore code or an invite, typed or pasted: several lines tall and monospaced, showing the code in groups of four with a space between, as Settings shows a restore code, whether it was typed in groups or run together.

**In the code:** macOS: `MacTextField` in a `FieldColumn` (`Views/Components/FieldColumn.swift`) for the label beside the field and the note under it; the box is `FieldChrome` `.boxed` (`Views/Components/MetadataForm/FieldChrome.swift`, applied by `.boxedTextField()`); `CommittedTextField` where an edit is kept on Return.

A label sits beside its field, aligned to the field's edge (right-aligned in left-to-right languages), as macOS lays out a column of labelled fields: every label ends the same distance from its box, however long the longest label is. A field on its own, not in a column of fields, takes `labelWidth: 'auto'`: its label is as wide as its words, so the gap to the box stays the same in every language.

A note or error under the field starts where the field's text starts, 10px in from its edge, as a grouped `Form` places a footer.
