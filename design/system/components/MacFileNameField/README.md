# MacFileNameField

A name pattern Save as… writes with: the pattern's pieces as chips, each with × to take it out, the pieces it can add, and a preview of a name it makes. Settings → Save as has two, labeled File name (each track's file) and Folder name (the folder a release save makes), in one box. One pattern for every format: formats differ in codec and quality, not in how files are named.

`label` sits above the field, at the leading edge where a row's title sits, and the field runs the box's full width; the Add and Preview lines follow under it.

**In the code:** macOS: `FilenameTokenEditor` (`Views/Settings/Formats/`), today inside each format's editor, with no label and no folder pattern.
