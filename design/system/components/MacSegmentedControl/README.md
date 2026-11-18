# MacSegmentedControl

For two or three choices side by side. `off` dims the whole control while none of its options can be chosen.

**In the code:** macOS: `Picker` with `.pickerStyle(.segmented)` (`InvitePersonSheet`, `StorageManagerView`, `ImportSearchFormView`, BaeKit's `TransferConcurrencyPicker`); `NativeSegmentedControl` (`Views/Library/AlbumDetail/NativeSegmentedControl.swift`) where a segment carries an image. (The title bar's Library/Import tabs are `MacMainWindow`'s, not this.)
