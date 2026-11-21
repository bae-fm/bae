# MacImportReleaseSummary

The release a folder holds, as the import pane shows it beside the folder's cover while bae imports it: the album title as a heading, the album artist under it, then the edition line and the copy line, in small tertiary text.

| Prop | What it does |
|---|---|
| `title` | The draft's album title. |
| `placeholder` | The draft has no title yet: the line says "Album title" in secondary text. |
| `artist` | The album artist; no line when the draft names none. |
| `edition` | The edition line of a catalog's release: its year, its country in the short form, each label with its catalog numbers. Left out when the draft comes from the files' own tags. |
| `copy` | The copy line: the media and details, then the audio the scan read, or the facts the files disagree on. |

**In the code:** macOS: `MacImportReleaseSummary` (`Views/Import/Mapping/MacImportReleaseSummary.swift`).
