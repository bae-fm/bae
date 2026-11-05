# MacBox

A rounded group of rows on `surface`, radius `radius-card`, with a `hairline` edge and a hairline between each child.

- Children: `MacRow`, `MacRadio` or `MacTextField` with `row`. Pass `empty` for the sentence shown when there are none: "No shared folders yet". No glyph above it.
- `elevated` floats it on `elevated` with a `shadow`.

**In the code:** macOS: the box a grouped `Form` draws around a `Section`; outside a `Form`, BaeKit's `.card(elevated:)`.
