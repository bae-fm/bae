import BaeKit
import SwiftUI

/// Presentation state shared by the image grid and its lightbox. Changing
/// layout never moves the focused cover or restarts an artwork lookup.
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
}

/// What the cover gallery draws, as core's read returns it: the saved cover,
/// each source's covers with the number of covers the source holds under the
/// filter, the number in all of them, and the focused cover with where it
/// sits. A source the filter hides is `nil`, and nothing is focused while the
/// gallery shows no cover.
struct CoverGalleryContent: Equatable {
    struct Section: Equatable {
        var items: [CoverItem]
        var count: Int
    }

    var current: CoverItem?
    var remote: Section?
    var files: Section?
    var total: Int
    var focus: BridgeGalleryFocus?

    static let empty = CoverGalleryContent(
        current: nil,
        remote: nil,
        files: nil,
        total: 0,
        focus: nil
    )

    /// The covers in the order the lightbox's strip shows them.
    var items: [CoverItem] {
        [current].compactMap { $0 } + (remote?.items ?? [])
            + (files?.items ?? [])
    }

    /// A picker's gallery as core read it.
    init(_ gallery: BridgeCoverGallery) {
        func section(_ section: BridgeCoverSection) -> Section {
            Section(
                items: section.items.map { CoverItem(gallery: $0) },
                count: Int(section.count)
            )
        }
        current = gallery.current.map { CoverItem(gallery: $0) }
        remote = gallery.remote.map(section)
        files = gallery.files.map(section)
        total = Int(gallery.total)
        focus = gallery.focus
    }

    private init(
        current: CoverItem?,
        remote: Section?,
        files: Section?,
        total: Int,
        focus: BridgeGalleryFocus?
    ) {
        self.current = current
        self.remote = remote
        self.files = files
        self.total = total
        self.focus = focus
    }
}
