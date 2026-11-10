# MacFieldRow

A row for a field. Use it in a `MacBox` alongside `MacRow`s; the box treats both as rows. A button beside the field (Save, Scan) takes the field's height and corners, so the pair reads as one control.

`alignText` when the field stands in for a row's name, as when naming a new folder in a list of folders: the field's text lines up with the rows' names, and its box sits outside them.

**In the code:** macOS: a labeled `TextField` in a grouped `Form`.
