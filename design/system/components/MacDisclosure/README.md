# MacDisclosure

For settings most people never change: "Advanced". Its own box: the chevron leads inside the label's row, centred in the leading column (`size-lead`) so it has room to turn, pointing right, and turns down when open; the label and the rows it shows start on that column's edge, the same edge as `MacItem`'s text and a nested row's. Children are `MacRow`s.

On Android the same thing is `AndroidDisclosure`.

**In the code:** macOS: `FormDisclosure` (`Views/Components/FormDisclosure.swift`) in a grouped `Form`.
