# MacTextField

A placeholder never repeats the label and never ends in an ellipsis. With a visible label, leave the empty field blank, unless the placeholder says what the field is for that the label can't: “Appears on your profile, ratings and playlists” under the label “Your name”. It has to fit the box on one line.

**In the code:** macOS: `TextField` in a grouped `Form`; `CommittedTextField` where an edit is kept on Return. The look drawn here is `FieldChrome` `.boxed` (`Views/Components/MetadataForm/FieldChrome.swift`).

A field on its own, not in a column of fields, takes `labelWidth: 'auto'`: its label is as wide as its words, so the gap to the box stays the same in every language.

A note or error under the field starts where the field's text starts, 10px in from its edge, as a grouped `Form` places a footer.
