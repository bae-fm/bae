# MacMenu

A menu, laid out as macOS lays one out, in bae's colours: the raised face (`elevated`) with a hairline edge, the highlighted item in the accent with the words on it, and a check beside the current choice. A pop-up's menu is `MacPopUpButton`'s `open`; a ⋯ menu is `MacMoreMenu`'s.

`more` draws a menu that runs past the screen's bottom edge: NSMenu's scroll arrow ends it, as the release editor's Country menu does.

**In the code:** macOS: NSMenu, the menu a `Picker` with `.pickerStyle(.menu)` or a `Menu` opens; the system draws it, and the app doesn't restyle it.
