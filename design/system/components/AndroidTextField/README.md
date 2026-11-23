# AndroidTextField

Android's outlined text field, used in dialogs: its label, an optional placeholder and a helper line. As Material's field does, an empty field without focus rests its label inside it, in the size of the text it takes, and shows no placeholder; focus or a value raises the label onto the outline, and the placeholder shows while the focused field is empty.

`code` is the field for a restore code or an invite, typed or pasted: several lines tall and monospaced, showing the code in groups of four with a space between, as Settings shows a restore code, whether it was typed in groups or run together.

`secret` shows the typed text as dots, for a password: a receiver's password in the pairing dialog.

**In the code:** Android: `AndroidTextField` (`ui/components/AndroidTextField.kt`), over Material's `OutlinedTextField`.
