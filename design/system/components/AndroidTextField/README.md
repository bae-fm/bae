# AndroidTextField

In dialogs.

A placeholder never repeats the label. A field with a label is blank when empty, unless the placeholder says what the field is for that the label can't: “Appears on your profile, ratings and playlists” under “Your name”. It has to fit the box on one line, about 40 characters on a phone.

`code` is the field for a restore code or an invite, typed or pasted: several lines tall and monospaced, showing the code in groups of four with a space between, as Settings shows a restore code, whether it was typed in groups or run together.

`secret` shows the typed text as dots, for a password: a receiver's password in the pairing dialog.

**In the code:** Android: Material's `OutlinedTextField`.
