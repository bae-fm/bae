# AndroidSegmentedControl

Material's single-choice segmented buttons, full width; the chosen one checked. `label` names the choice over them, in the secondary text, as a form does: Role, Invite lasts. `helper` says what the chosen option means, under the buttons in smaller secondary text, as the playlist dialog's Who sees it does.

**In the code:** Android: `AndroidSegmentedButtons` (`ui/components/AndroidSegmentedButtons.kt`), on Material's `SingleChoiceSegmentedButtonRow`. The playlist dialog (`ui/library/PlaylistDialogParts.kt`) draws the helper line as a `Text` under it.
