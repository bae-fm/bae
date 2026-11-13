# MacMoreMenu

A row's ⋯ button with the row's actions in its menu: a row offers its actions here, not as a button of each. `open` draws the menu under the button, `items` its entries in order ("-" for a line between groups), `highlighted` the one under the pointer, `off` dims and disables it while none of its actions can run (the other libraries' while one is being removed). `label` names the button (More unless given; an artist page's is Artist options). A library's row on Welcome, a format, a person, a device, a cast receiver, an artist's page and a playlist's.

**In the code:** macOS: `MacMoreMenu` (`Views/Components/MacMoreMenu.swift`), in `CastReceiverRow`, `FormatsSettingsTab`, `PeopleAndDevicesPage` and `LocalLibrariesSection`.
