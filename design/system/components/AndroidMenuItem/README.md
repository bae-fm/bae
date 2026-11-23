# AndroidMenuItem

One line of an `AndroidMenu`. `selected` for the current choice; `check` (true or false) for an on/off item, its check shown when on; `heading` for a group's name; `divider` for a line. `pressed` for the item under the finger: the `pressed` state layer over it (a phone has no hover). `label` names an item that shows a picture in place of words, such as a row of stars, for a screen reader.

**In the code:** Android: `AndroidMenuItem` (`ui/library/AndroidMenuItem.kt`) for a choice, Material's `DropdownMenuItem` for an action.
