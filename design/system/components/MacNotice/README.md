# MacNotice

macOS's notice: a tinted box in its tone, the tone's glyph at the top left. It says something about part of a screen: an error, a warning, information, a success. A title says it; a subtitle, when there is one, says what to do, and then the title is bold. A notice never shows the error itself: no Details, ever. When there is an error behind it, clicking the notice opens `MacAlert` with `details`, with the explanation and the causes. A view that failed whole is not a notice: see the load failure.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default, with no glyph). |
| `title` | What happened. |
| `subtitle` | What to do about it. |
| `action` | The label of a button (Retry) on its own line at the end, in line with the text. |
| `quiet` | Drawn as a row: no fill, the screen's text, the glyph in its tone in its outline form. For a list of problems, such as the Activity panel's Needs Attention. |
| `dismiss` | A dim × at the right, level with the title: takes the notice away without fixing what it reports. For the Activity panel's problems, each of which still shows where it's fixed (such as Settings → Sync). |
| `icon` | On unless `false`. |
| children | Other content, in place of or after the title. |

Its text takes the size of the screen it sits in. The glyph is 1.19 times the text, centred on the first line. Info, success, warning and danger fill with their own notice colour (`info-notice` …), with `on-notice` text; neutral keeps the neutral tint and the screen's text.

**In the code:** macOS: BaeKit's `.notice(_:)`, and `ErrorDetailDisclosure` (`BaeKit/Sources/BaeKit/Views/ErrorDetailDisclosure.swift`), which becomes the alert.
