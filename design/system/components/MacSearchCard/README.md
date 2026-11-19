# MacSearchCard

Search's results, in a 440pt card hanging under the field with their right edges lined up, up to 455pt tall. Sections in a fixed order: Albums, Artists, Tracks, Composers, Works; a kind with nothing found has no section. `empty`: the search found nothing, and the card says No matches as a `MacContentUnavailable` title. Shown only once results arrive; the last results stay while the next query runs.

**In the code:** macOS: `SearchView` (`Views/Search/SearchView.swift`), placed by `MainAppView`.
