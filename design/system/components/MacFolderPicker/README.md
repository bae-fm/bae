# MacFolderPicker

The folder picker for a Dropbox or OneDrive library: the folders to choose an empty one from. The setup sheet puts its `MacSheetFooter` under the picker, with New folder… and Look again on the left (`split="2"`) and Cancel and Set up on the right; New folder… and Look again are off while the person names a folder, and Set up is off until an empty folder is chosen.

| Prop | |
| --- | --- |
| `provider` | `dropbox` (the default): the shared folders, in one list. `onedrive`: the folders in the open folder, under its path with Up, each with a chevron that opens it. |
| `state` | `naming` (the default): the person types a new folder's name. `list`: a folder is chosen. `made`: bae made the new folder and chose it. `notEmpty`: the chosen folder holds files, and a notice says to choose an empty one. `empty`: there are no folders. |

**In the code:** macOS: `MacFolderPicker` (`bae-macos/bae/bae/Views/Settings/Library/MacFolderPicker.swift`); `OAuthFolderSetup`'s `SetupStep` draws the footer.
