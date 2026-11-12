# MacImportDestination

Where an import goes, as one pop-up button of two choices:

- **This Mac:** the files stay on this Mac only.
- **Cloud:** the files go to the cloud, so every device can play them.

It replaces the In cloud and Pinned checkboxes. There is no choice to keep a copy here as well: an imported release in the cloud is pinned the way any release is, from its Storage sheet. It is one stored choice, the one the person last made, shown in three places: beside Import on the import card, on the bulk pane's Import row, and in Settings → Import as "Import to" under Import automatically, where `off` dims it while automatic import is off. Changing it in one place changes it in all. It shows only when the library has cloud storage.

**In the code:** macOS: `MacImportDestination` (`Views/Import/MacImportDestination.swift`), in `ImportReleaseHeader`, `ImportCandidateBulkSelectionPane` and `ImportSettingsTab`.
