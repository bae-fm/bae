# MacFormField

A labelled control in a sheet's form: its label (`label`) above the control, and a note (`note`) under it, all on the form's one left edge. The control takes the field's label as its name, which VoiceOver reads, and draws no label of its own. A text field inside it runs the form's width; any other control, such as a `MacSegmentedControl`, keeps its own width. Stack several with the sheet's spacing between them.

**In the code:** macOS: `MacFormField` (`Views/Components/MacFormField.swift`).
