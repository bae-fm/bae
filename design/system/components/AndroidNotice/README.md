# AndroidNotice

Android's notice: a tinted box in its tone, the tone's glyph at the top left. It says something about part of a screen: an error, a warning, information, a success. A title says it; a subtitle, when there is one, says what to do, and then the title is bold. A notice never shows the error itself: no Details, ever. When there is an error behind it, the notice's secondary button is Details, which opens an `AndroidDialog` holding `AndroidErrorDetails`, with the explanation and the causes: errors and technical words stay off the app's main screens. A view that failed whole is a notice too, in the centred layout.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default, with no glyph). |
| `title` | What happened. |
| `subtitle` | What to do about it. |
| `action` | The primary button's label (Retry). |
| `quiet` | Drawn as a row: no fill, the screen's text, the glyph in its tone in its outline form. For a list of problems, such as the Activity panel's Needs Attention. Also inside an alert or dialog, always: a filled box there stands out from everything else in it.|
| `secondary` | The secondary button's label: Dismiss, or Details (opens the error's causes in a popup). Secondary comes first, before the primary. |
| `layout` | `paragraph` (the default) or `centered`: in the middle of the space it's given, the glyph (an outline, in grey) in front of the title, the title in primary text, the buttons centred under it. For a view that failed whole (the album grid, the import list, a composer's page, the lightbox's image), or a part of one (the rest of the queue). |
| `buttons` | Where the buttons go in the paragraph layout: `inline` (the default), on the text's line at the right, centred on it, the text wrapping beside them; or `below`, on a line of their own under the text, in line with it, for a long button in a narrow place (the Activity panel's Open Sync Settings). The centred layout always puts them in one row under the title. |
| `dismiss` | An × at the right, level with the title: takes the notice away without fixing what it reports. For the Activity panel's problems, each of which still shows where it's fixed (such as Settings → Sync). |
| `icon` | On unless `false`. |
| children | Other content, in place of or after the title. |

Its text takes the size of the screen it sits in. The glyph is 1.19 times the text, centred on the first line. In light, info, success, warning and danger fill with their own pale notice colour (`info-notice` …), with `on-notice` text. In dark, every notice has the neutral fill and the screen's text, and only the glyph carries the tone: a coloured fill over a dark screen reads grey or brown when see-through and glares when solid.

**In the code:** Android: `AndroidNotice` (`ui/components/AndroidNotice.kt`), two overloads: one taking the error and drawing its own line, one taking a title, advice and an optional error. Both draw the framed layout on `NoticeFrame` (`ui/components/Status.kt`, with `NoticeLook` for filled and quiet) and the centred layout (`NoticeLook.CENTERED`) for a view that failed whole. Details opens `ErrorDetailsDialog` (`ui/components/ErrorDetails.kt`), holding the explanation and `AndroidErrorDetails`; the dialog's one button is the system's OK. `onDismiss` adds the ×.
