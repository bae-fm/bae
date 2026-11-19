# MacTitleBar

The main window's title bar: the section tabs (`MacSectionTabs`) in the middle, then the search field, the Activity button (`MacActivityButton`) and the avatar (`MacAvatarButton`) at the end. The window's traffic lights sit at its start.

| Prop | What it does |
|---|---|
| `section` | Library or Import, the section showing. |
| `search`, `searchFocused` | What the person typed in the search field, and whether the field has focus. |
| `sync` | The Activity button's state: moving, attention or paused. |
| `avatar`, `avatarImage` | The person's name and picture. |

**In the code:** macOS: `TitleBar` (`Views/Chrome/TitleBar.swift`, and `baeShots/ShotScene.swift` for the shots).
