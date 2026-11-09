# MacNotice

macOS's notice: a tinted box in its tone, the tone's glyph at the top left. It says something about part of a screen: an error, a warning, information, a success. A title says it; a subtitle, when there is one, says what to do, and then the title is bold. A notice never shows the error itself: no Details, ever. When there is an error behind it, the notice's secondary button is Details, which opens `MacAlert` with `details`, with the explanation and the causes: errors and technical words stay off the app's main screens. A view that failed whole is a notice too, in the centred layout.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default, with no glyph). |
| `title` | What happened. |
| `subtitle` | What to do about it. |
| `action` | The primary button's label (Retry). Alone, a button (Retry) on its own line under the text, in line with it. |
| `quiet` | Drawn as a row: no fill, the screen's text, the glyph in its tone in its outline form. For a list of problems, such as the Activity panel's Needs Attention. |
| `secondary` | The secondary button's label: Dismiss, or Details (opens the error's causes in a popup). With `action`, the two go in the sheet footer's row at the right (`MacSheetFooter`), secondary first; centred, in one row under the title. |
| `layout` | `paragraph` (the default) or `centered`: in the middle of the space it's given, the glyph (an outline, in grey) in front of the title, the title in secondary text, the buttons centred under it. For a view that failed whole (the album grid, the import list, a composer's page, the lightbox's image), or a part of one (the rest of the queue). |
| `dismiss` | A dim × at the right, level with the title: takes the notice away without fixing what it reports. For the Activity panel's problems, each of which still shows where it's fixed (such as Settings → Sync). |
| `icon` | On unless `false`. |
| `stage` | On the lightbox's dark stage, in its text colours. |
| children | Other content, in place of or after the title. |

Its text takes the size of the screen it sits in. The glyph is 1.19 times the text, centred on the first line. Info, success, warning and danger fill with their own notice colour (`info-notice` …), with `on-notice` text; neutral keeps the neutral tint and the screen's text.

**In the code:** macOS: BaeKit's `.notice(_:)`; `ErrorNotice` (`BaeKit/Sources/BaeKit/Views/ErrorNotice.swift`), which becomes the Details button and its alert; and `LoadFailureView` (`BaeKit/Sources/BaeKit/Views/LoadFailureView.swift`), which becomes the centred layout.
