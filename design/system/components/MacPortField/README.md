# MacPortField

A port number for a server bae runs (Subsonic, Automation): a field five digits wide, at the row's trailing edge like any control, its digits starting at the field's left as in any text field. It takes digits only; anything else typed is ignored. Its label is hidden, since the row says Port.

It saves on Return or on leaving the field, like every settings field. What's wrong with it goes on the row's second line in red, with the field red: “Use a number from 1 to 65535” for one out of range, “Another app is using this port” when the server can't have it. A port change restarts the server, and the address shown with the new port is the confirmation.

**In the code:** macOS: none yet; Subsonic and Automation each use a `TextField` 88 points wide with a Save button.
