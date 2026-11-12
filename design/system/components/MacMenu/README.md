# MacMenu

A menu, laid out as macOS lays one out, in bae's colours: the raised face (`elevated`) with a hairline edge, the highlighted item in the accent with the words on it, and a check beside the current choice. A pop-up's menu is `MacPopUpButton`'s `open`; a ⋯ menu is `MacMoreMenu`'s.

**In the code:** macOS: NSMenu, the menu a `Picker` with `.pickerStyle(.menu)` or a `Menu` opens; the system draws it, and the app doesn't restyle it.
