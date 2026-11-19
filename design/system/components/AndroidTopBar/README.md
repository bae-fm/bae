# AndroidTopBar

A screen's bar, 64dp tall on the page's own colour: the back button (`back`, named to TalkBack by `backLabel`, such as "Back to Mara Quill"), the title (`title`), and at the end either `menu`, a choice that applies to the whole screen (History's whose plays) as its current value with a ⌄, which opens a menu of the choices, or `more`, the ⋮ that opens the screen's menu, named by its label ("Album options"). `AndroidScreen` draws it when the screen has a title. `AndroidDetailScreen` draws it with no title: back, and the ⋮ when the page has a menu.

**In the code:** Android: `AndroidTopBar` (`ui/components/AndroidScreen.kt`), with the screen's own actions at its end.
