# MacPopUpButton

A pop-up, laid out as macOS lays one out, in bae's colours: the chosen value as text, then a round button in the control fill (`control`) holding the up-and-down chevron. For a choice of more than three; fewer read better as `MacRadio` rows. `label` names it by what it chooses (Import to, Album), as the app's Picker is named by its title; the value it shows is its value.

`open` draws its menu as macOS opens it: over the control, the chosen item on the button's line with its words over the value's, checked and highlighted. `items` lists the choices ("-" for a line), `chosen` names the checked one when its words differ from the value (Wi-Fi's address), `highlighted` the one under the pointer. On a board whose window would crop the menu, `menuInside` and `menuTop` move it, as macOS moves a menu to keep it on the screen.

**In the code:** macOS: `Picker` with `.pickerStyle(.menu)`; the system draws it and its menu, which the app doesn't restyle.
