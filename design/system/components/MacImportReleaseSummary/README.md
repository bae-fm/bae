# MacImportReleaseSummary

The release a folder holds, as the import pane shows it beside the folder's cover while bae imports it: the album title as a heading, the album artist under it, then the facts line and the audio the scan read, in small tertiary text.

| Prop | What it does |
|---|---|
| `title` | The draft's album title. |
| `placeholder` | The draft has no title yet: the line says "Album title" in secondary text. |
| `artist` | The album artist; no line when the draft names none. |
| `facts` | The release's facts on one line: media, year, country, the first catalog number and the track count for a catalog's release; "From file metadata" and the count when the draft comes from the files' own tags. |
| `audio` | The audio the scan read: the format, or the facts the files disagree on. |

**In the code:** macOS: `MacImportReleaseSummary` (`Views/Import/Mapping/MacImportReleaseSummary.swift`).
