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

        func includes(_ item: CoverItem) -> Bool {
            switch (self, item.selection) {
            case (.all, _): true
            case (.releaseFiles, .releaseImage),
                (.releaseFiles, .embeddedCover):
                true
            case (.discogs, .remoteCover(let cover)): cover.source == .discogs
            case (.musicBrainz, .remoteCover(let cover)):
                cover.source == .musicBrainz
            default: false
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

/// What the cover gallery draws: the saved cover, each source's covers with
/// the number of covers the source holds under the filter, and the number in
/// all of them. A source the filter hides is `nil`. Core fills it for a
/// library release; a candidate's is filtered here until its list moves to
/// core too.
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

    /// A library release's gallery as core read it.
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

    /// A candidate's remote and folder covers, filtered by `filter`.
    init(
        remote: [CoverItem],
        files: [CoverItem],
        filter: ArtworkBrowserState.Filter
    ) {
        let shownRemote = remote.filter(filter.includes)
        let shownFiles = files.filter(filter.includes)
        current = nil
        self.remote =
            filter == .releaseFiles
            ? nil : Section(items: shownRemote, count: shownRemote.count)
        self.files =
            filter == .all || filter == .releaseFiles
            ? Section(items: shownFiles, count: shownFiles.count) : nil
        total = shownRemote.count + shownFiles.count
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
