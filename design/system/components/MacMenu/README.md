# MacMenu

macOS's own menu, as the system draws it: its face (`mac-system-surface`) with a hairline edge, the highlighted item in the system's accent (`mac-system-accent`) with white words, whatever accent the app has, and a check beside the current choice. A pop-up's menu is `MacPopUpButton`'s `open`; a ⋯ menu is `MacMoreMenu`'s.

**In the code:** macOS: NSMenu, the menu a `Picker` with `.pickerStyle(.menu)` or a `Menu` opens; the system draws it, and the app doesn't restyle it.
