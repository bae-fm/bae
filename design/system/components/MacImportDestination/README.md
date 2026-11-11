# MacImportDestination

Where an import goes, as one pop-up button of three choices:

- **This Mac:** the files stay on this Mac only.
- **Cloud:** the files go to the cloud, so every device can play them.
- **Cloud, pinned:** the files go to the cloud, and a whole copy stays on this Mac too.

It replaces the In cloud and Pinned checkboxes, which also allowed pinned without the cloud, a choice that means nothing. It is one stored choice, the one the person last made, shown in three places: beside Import on the import card, on the bulk pane's Import row, and in Settings → Import as "Import to" under Import automatically, where `off` dims it while automatic import is off. Changing it in one place changes it in all. It shows only when the library has cloud storage.

**In the code:** macOS: `ImportDestinationPicker` (`Views/Import/ImportDestinationPicker.swift`), in `ImportReleaseHeader`, `ImportCandidateBulkSelectionPane` and `ImportSettingsTab`.
