# MacTextField

A placeholder never repeats the label and never ends in an ellipsis. With a visible label, leave the empty field blank.

**In the code:** macOS: `TextField` in a grouped `Form`; `CommittedTextField` where an edit is kept on Return. The look drawn here is `FieldChrome` `.boxed` (`Views/Components/MetadataForm/FieldChrome.swift`).

A note or error under the field starts where the field's text starts, 10px in from its edge, as a grouped `Form` places a footer.
