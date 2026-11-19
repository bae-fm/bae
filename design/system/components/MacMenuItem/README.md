# MacMenuItem

One line of a `MacMenu`: a choice, a heading over a group of choices, a divider between groups, or a submenu.

| Prop | What it does |
|---|---|
| `checked` | A ✓ before the words: the choice in force. |
| `highlighted` | The line under the pointer, on the accent. |
| `off` | A choice that can't be made now, in tertiary text. |
| `heading` | The words head the group below, in small secondary text, with no ✓ column. |
| `divider` | A line between groups; the item has no words. |
| `submenu` | A › at the line's end opens more items. |
| `accessory` | On a submenu, a glyph before the ›: `problem` (a red ! in a circle, for a folder whose scan failed or that bae is not watching), `scanning` (a spinner) or `network` (a globe, for a folder on a network volume). |
| `help` | The accessory's hover text: the problem's words, or how bae checks a network folder. |

**In the code:** macOS: BaeKit `MacMenuItem`, `MacMenuDivider`, `MacMenuSection` (its heading) and `MacMenuSubmenu` with its `MacMenuAccessory` (`BaeKit/Sources/BaeKit/Views/MacMenu.swift`), drawn by `MacMenuView`.
