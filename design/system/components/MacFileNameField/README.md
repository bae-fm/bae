# MacFileNameField

A name pattern Save as… writes with: the pattern's values as chips, each with × to take it out, and any text typed between them (the ` - ` in Artist - Album), a preview of a name it makes, and the values it can add. A value the release doesn't state comes out empty and the text around it stays. Typed text is kept as typed, spaces and all.

Typed text that can't make a name turns the field red, and the preview line becomes the error: “A name can’t start with “.”.”, “A name can’t contain “/”.” (naming the first of `\ : ? * " < > | /` it holds), “A name can’t be empty.”. Saves name by the default pattern until it's fixed; the error doesn't say so. Settings → Save as has two, labeled File name (each track's file) and Folder name (the folder a release save makes), in one box. One pattern for every format: formats differ in codec and quality, not in how files are named.

`pattern` is the field's text with each value in brackets: `[Artist] - [Album]`. `label` sits above the field, at the leading edge where a row's title sits, and the field runs the box's full width; the preview sits right under it as the field's help line, then the Add line.

**In the code:** macOS: `SaveNameField` (`Views/Settings/Formats/`), today chips only, joined with spaces: no typed text between them, and no album year.
