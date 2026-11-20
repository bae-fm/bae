# MacInlineField

A field drawn as the value it holds until the pointer is over it or it has focus; then the field's fill shows. The draft card's values and a track's title and artist are these fields. The field pulls out by its padding, so its words keep the column's edge.

- `label` names the field; `value` is the text it holds; `placeholder` shows while it is empty.
- `large` sets the field in the album title's size.
- `quiet` draws its value in the tertiary colour, as the album year.
- `fit` makes the field as wide as its value, and 96pt while it is empty, for a field in a list row, as a label's name and catalog number.
- `mono` sets its value in the mono face, as a catalog number or a barcode.

**In the code:** macOS: `FieldChrome` (`bae-macos/bae/bae/Views/Components/MetadataForm/FieldChrome.swift`) on `CommittedTextField` (`bae-macos/bae/bae/Views/Components/CommittedTextField.swift`)
