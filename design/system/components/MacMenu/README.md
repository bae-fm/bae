# MacMenu

A menu, laid out as macOS lays one out, in bae's colours: the raised face (`elevated`) with a hairline edge, the highlighted item in the accent with the words on it, and a check beside the current choice. The menu is as wide as its longest item and never wraps a line, as macOS sizes a menu. A pop-up's menu is `MacPopUpButton`'s `open`; a ⋯ menu is `MacMoreMenu`'s.

A menu taller than the screen fits the screen and scrolls its lines, and draws no scroll arrow at its foot, as bae's menu panel does.

**In the code:** macOS: `MacMenuFace` in BaeKit (`MacMenuView.swift`), which `MacMenuSession` opens in a borderless panel; shots draw it in place with `MacMenuInPlace`.
