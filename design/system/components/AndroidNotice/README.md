# AndroidNotice

Android's notice: a tinted box in its tone, the tone's glyph at the top left. It says something about part of a screen: an error, a warning, information, a success. A title says it; a subtitle, when there is one, says what to do, and then the title is bold. A notice never shows the error itself: no Details, ever. When there is an error behind it, the notice's secondary button is Details, which opens an `AndroidDialog` holding `AndroidErrorDetails`, with the explanation and the causes: errors and technical words stay off the app's main screens. A view that failed whole is a notice too, in the centred layout.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default, with no glyph). |
| `title` | What happened. |
| `subtitle` | What to do about it. |
| `action` | The primary button's label (Retry). Alone, a text button (Retry) on its own line under the text, in line with it. |
| `quiet` | Drawn as a row: no fill, the screen's text, the glyph in its tone in its outline form. For a list of problems, such as the Activity panel's Needs Attention. |
| `secondary` | The secondary button's label: Dismiss, or Details (opens the error's causes in a popup). With `action`, the two go in the dialog's actions row at the right (`AndroidDialogActions`), secondary first; centred, in one row under the title. |
| `layout` | `paragraph` (the default) or `centered`: in the middle of the space it's given, the glyph (an outline, in grey) in front of the title, the title in secondary text, the buttons centred under it. For a view that failed whole (the album grid, the import list, a composer's page, the lightbox's image), or a part of one (the rest of the queue). |
| `dismiss` | An × at the right, level with the title: takes the notice away without fixing what it reports. For the Activity panel's problems, each of which still shows where it's fixed (such as Settings → Sync). |
| `icon` | On unless `false`. |
| children | Other content, in place of or after the title. |

Its text takes the size of the screen it sits in. The glyph is 1.19 times the text, centred on the first line. Info, success, warning and danger fill with their own notice colour (`info-notice` …), with `on-notice` text; neutral keeps the neutral tint and the screen's text.

**In the code:** Android: `Notice` (`ui/components/Status.kt`, with `NoticeLook` for filled and quiet); `ErrorNotice` for an error's own line and `TitledErrorNotice` for a title with advice (`ui/components/ErrorNotice.kt`), which also draw the centred layout (`NoticeLook.CENTERED`) for a view that failed whole. Details opens `ErrorDetailsDialog` (`ui/components/ErrorDetails.kt`), holding the explanation and the causes panel with its copy button; the dialog's one button is the system's OK. `onDismiss` adds the ×.
