# AndroidErrorDisclosure

Android's error box: a danger `Notice` with Material's glyph, the message in primary text, then a line “Details” with `expand_more` (16) after the word. Open, it shows `expand_less` and the causes in the same inset panel as macOS, one a line and ending in the fault, with Copy details at its corner.

| Prop | |
| --- | --- |
| `tone` | `danger` (the default), `warning`, or `neutral` for a reason. |
| `fault` | The deepest cause; with it there is a Details. |
| `open`, `chain` | Details open; `chain` is the causes, one a line (without it, the fault alone). |
| `action` | The label of a text button (Retry) at the box's right, level with the message. |
| `bare` | No notice, for a disclosure inside a box that already holds it. |

**In the code:** Android: `ErrorDetailDisclosure` (`ui/components/ErrorDetailDisclosure.kt`).
