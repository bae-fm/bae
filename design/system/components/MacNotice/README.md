# MacNotice

macOS's notice: a tinted box in its tone, the tone's glyph at the top left. It says something about part of a screen: an error, a warning, information, a success. A title says it; a subtitle, when there is one, says what to do, and then the title is bold. A notice never shows the error itself: no Details, ever. When there is an error behind it, the notice's secondary button is Details, which opens `MacAlert` with `details`, with the explanation and the causes: errors and technical words stay off the app's main screens. A view that failed whole is a notice too, in the centred layout.

| Prop | |
| --- | --- |
| `tone` | `info`, `success`, `warning`, `danger` or `neutral` (the default, with no glyph). |
| `title` | What happened. |
| `subtitle` | What to do about it. |
| `action` | The primary button's label (Retry). |
| `quiet` | Drawn as a row: no fill, the screen's text. For a list of problems, such as the Activity panel's Needs Attention. Also inside an alert or dialog, always: a filled box there stands out from everything else in it.|
| `secondary` | The secondary button's label: Dismiss, or Details (opens the error's causes in a popup). Secondary comes first, before the primary. |
| `leave` | Centred only: the label of a button that leaves the screen the notice fills (Cancel), first in the row, where the notice is the screen's only way back: a failed Reset from cloud. |
| `layout` | `paragraph` (the default) or `centered`: in the middle of the space it's given, the glyph (an outline, in grey) in front of the title, the title in secondary text when it stands alone and in primary over a subtitle, the button's label in secondary text and Details in the accent, centred under it. For a view that failed whole (the album grid, the import list, a composer's page, the lightbox's image), or a part of one (the rest of the queue). |
| `buttons` | Where the buttons go in the paragraph layout: `inline` (the default), on the text's line at the right, centred on it, the text wrapping beside them; or `below`, on a line of their own under the text, in line with it, for a long button in a narrow place (the Activity panel's Open Sync Settings). The centred layout always puts them in one row under the title. |
| `dismiss` | A dim × at the right, level with the title: takes the notice away without fixing what it reports. For the Activity panel's problems, each of which still shows where it's fixed (such as Settings → Sync). |
| `icon` | On unless `false`. |
| `stage` | On the lightbox's dark stage, in its text colours. |
| children | Other content, in place of or after the title. |

Inside a `MacBox` a filled notice keeps to the rows' edges: it is inset by the rows' padding, never out to the box's sides.

Its text takes the size of the screen it sits in. The glyph is 1.19 times the text, centred on the first line. In light, info, success, warning and danger fill with their own pale notice colour (`info-notice` …), with `on-notice` text. In dark, every notice has the neutral fill and the screen's text, and only the glyph carries the tone: a coloured fill over a dark screen reads grey or brown when see-through and glares when solid.

Its text buttons (Details) are the accent, so they read apart from the title. Every notice's glyph is the outline form, in the tone's colour (grey in the centred layout): a filled disc is louder than the words beside it.

### Writing one

A library on the Mac's Welcome list fails to open because its tables don't match the app's.

- Before, a notice over the whole list: "Library failed to open / Choose another library, or restore it with “Enter code”." It doesn't say which library, and Enter code isn't on the screen.
- Rejected: "Couldn’t open “Home music”" (the row already names it); "This Mac’s copy can’t be read" (copy of what?); "It doesn’t match what this version of bae reads … the library in the cloud is untouched." (the mechanism); "The library is safe in the cloud." (a claim nobody checked, in an advert's voice); "Load it again" (a name the app doesn't use); "Open another library" (true, and no help); "Edits that haven’t synced are lost." (the confirm says it).
- Final, joined under the library's row: the title Could not open, the line "Something is wrong with this library’s data.", and the buttons Details and Reset from cloud…, whose confirm warns about edits that haven't synced.

**In the code:** macOS: `Notice` (`BaeKit/Sources/BaeKit/Views/Notice.swift`), whose Details button opens an alert and whose centred look draws a view that failed whole.
