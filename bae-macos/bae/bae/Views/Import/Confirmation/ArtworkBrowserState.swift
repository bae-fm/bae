import BaeKit
import SwiftUI

/// Presentation state shared by the image grid and its lightbox. Changing
/// layout never rebuilds the selection or restarts an artwork lookup.
struct ArtworkBrowserState {
    enum Layout {
        case grid, lightbox
    }

    enum Filter: Hashable, CaseIterable {
        case all, releaseFiles, discogs, musicBrainz

        var label: String {
            switch self {
            case .all: String(localized: "All")
            case .releaseFiles: String(localized: "Release files")
            case .discogs: bridgeCatalogName(catalog: .discogs)
            case .musicBrainz: bridgeCatalogName(catalog: .musicBrainz)
            }
        }

        /// The filter as core's read takes it.
        var bridge: BridgeCoverFilter {
            switch self {
            case .all: .all
            case .releaseFiles: .releaseFiles
            case .discogs: .catalog(catalog: .discogs)
            case .musicBrainz: .catalog(catalog: .musicBrainz)
            }
        }
    }

    var layout: Layout
    var cursor: Cursor<CoverItem>?

    init(layout: Layout) {
        self.layout = layout
    }

    /// Show `content`, keeping the cover the cursor is on, else the saved
    /// cover, else the one selected, when the new content still holds it.
    mutating func update(
        content: CoverGalleryContent,
        selectedCover: BridgeCoverSelection?
    ) {
        let preferred =
            cursor?.current.id ?? content.current?.id
            ?? selectedCover.map(CoverItem.ID.selection)
        cursor = Cursor(items: content.items, preferring: preferred)
    }
}

/// What the cover gallery draws, as core's read returns it: the saved cover,
/// each source's covers with the number of covers the source holds under the
/// filter, and the number in all of them. A source the filter hides is `nil`.
struct CoverGalleryContent: Equatable {
    struct Section: Equatable {
        var items: [CoverItem]
        var count: Int
    }

    var current: CoverItem?
    var remote: Section?
    var files: Section?
    var total: Int

    static let empty = CoverGalleryContent(
        current: nil,
        remote: nil,
        files: nil,
        total: 0
    )

    /// The covers in the order the gallery's cursor walks them.
    var items: [CoverItem] {
        [current].compactMap { $0 } + (remote?.items ?? [])
            + (files?.items ?? [])
    }

    /// A picker's gallery as core read it.
    init(_ gallery: BridgeCoverGallery) {
        func section(_ source: BridgeCoverSectionSource) -> Section? {
            gallery.sections.first { $0.source == source }
                .map {
                    Section(
                        items: $0.items.map { CoverItem(gallery: $0) },
                        count: Int($0.count)
                    )
                }
        }
        current = gallery.current.map { CoverItem(gallery: $0) }
        remote = section(.remote)
        files = section(.releaseFiles)
        total = Int(gallery.total)
    }

    private init(
        current: CoverItem?,
        remote: Section?,
        files: Section?,
        total: Int
    ) {
        self.current = current
        self.remote = remote
        self.files = files
        self.total = total
    }
}
