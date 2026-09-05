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
            case .releaseFiles: String(localized: "Release Files")
            case .discogs: bridgeMetadataSourceName(source: .discogs)
            case .musicBrainz: bridgeMetadataSourceName(source: .musicBrainz)
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
    private(set) var filter: Filter = .all
    private var savedCover: CoverItem?
    private var remote: [CoverItem] = []
    private var files: [CoverItem] = []
    var currentCover: CoverItem? { filter == .all ? savedCover : nil }
    var remoteItems: [CoverItem] { remote.filter(filter.includes) }
    var releaseItems: [CoverItem] { files.filter(filter.includes) }
    var showsRemoteSources: Bool { filter != .releaseFiles }
    var showsReleaseFiles: Bool { filter == .all || filter == .releaseFiles }
    var cursor: Cursor<CoverItem>?
    private var allItems: [CoverItem] {
        (savedCover.map { [$0] } ?? []) + remote + files
    }

    init(layout: Layout) {
        self.layout = layout
    }

    mutating func update(
        currentCover: CoverItem?,
        remoteItems: [CoverItem],
        releaseItems: [CoverItem],
        selectedCover: BridgeCoverSelection?
    ) {
        let preferred =
            cursor?.current.id ?? currentCover?.id
            ?? selectedCover.map(CoverItem.ID.selection)
        savedCover = currentCover
        remote = remoteItems
        files = releaseItems
        cursor = Cursor(
            items: allItems.filter(filter.includes),
            preferring: preferred
        )
    }

    mutating func setFilter(_ filter: Filter) {
        let preferred = cursor?.current.id
        self.filter = filter
        cursor = Cursor(
            items: allItems.filter(filter.includes),
            preferring: preferred
        )
    }
}
