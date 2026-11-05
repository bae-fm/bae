# Sheet

A sheet that drops from the window's title bar, on `elevated` with `radius-card` lower corners: a title in `heading`, a `hairline`, then its content, and a `SheetFooter` last.

- Props: `title`, `overlay`, `width` (460).
- `overlay` dims the window under it with `scrim` and sits it at the top; the element around it must be `position: relative`.
- Cancel goes back one screen, and closes the sheet on the first one. No Back buttons.
