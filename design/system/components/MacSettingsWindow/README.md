# MacSettingsWindow

The macOS Settings window: the pane list on the left with `pane` chosen, the pane on the right.

- Props: `pane`, `title` (the pane's heading), `back` (the pane a Back button returns to, for a page inside a pane), `width` and `height` (690 × 600).
- Children are the pane's content: `MacSection`s over `MacBox`es. A `MacSheet` with `overlay` inside covers the whole window.
- The window is `background`, the pane list `surface`, the chosen pane `accent-tint-strong`.
