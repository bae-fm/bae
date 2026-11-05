# MacSheet

A sheet that drops from the window's title bar, on `elevated`: a title in `heading`, a `hairline`, then its content, and a `MacSheetFooter` last.

- Props: `title`, `overlay`, `width` (460). `overlay` dims what is under it with `scrim`; inside a `MacSettingsWindow` it covers the window.
- Cancel goes back one screen, and closes the sheet on the first one. No Back buttons.
