# AndroidMoreMenu

A row's ⋮ button with the row's actions in its menu: a row offers its actions here, not as a button of each. `open` draws the menu under the button, `items` its entries in order ("-" for a line between groups), `highlighted` the one under the pointer. `off` dims the ⋮ while none of its actions can run: on Welcome, the other libraries' ⋮ while one library is being removed, since one removal runs at a time. A library's row on Welcome, a format, a person, a device, a cast receiver.

**In the code:** Android: `MoreMenu` (`ui/components/MoreMenu.kt`), in `WelcomeLibrariesSection`. The ⋮ the boards draw on a person's page and on Devices is not built: those pages' rows open a page (`ui/settings/PeoplePages.kt`).
