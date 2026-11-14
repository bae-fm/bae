import BaeKit
import Foundation

/// A selectable candidate image or a persisted release file, retaining the
/// source's identity for both previewing and saving.
struct CoverItem: LightboxImage {
    static func == (lhs: CoverItem, rhs: CoverItem) -> Bool {
        lhs.id == rhs.id
    }

    enum Identity: Hashable {
        case selection(BridgeCoverSelection)
        case currentCover
    }

    var id: Identity {
        if let selection { return .selection(selection) }
        return .currentCover
    }

    var selection: BridgeCoverSelection? {
        switch content {
        case .candidate(let choice): choice.selection
        case .library(let cover): cover.selection
        }
    }
    var image: ImageContent {
        switch content {
        case .candidate(let choice): choice.imageContent
        case .library(let cover): ImageContent(bridge: cover.image)
        }
    }

    var sourceLabel: String {
        switch selection {
        case .remoteCover(let remote):
            bridgeCatalogName(catalog: remote.source)
        case .releaseImage: String(localized: "Release files")
        case .embeddedCover:
            localizedCoreString("ui.import.metadata.file_metadata")
        case nil: String(localized: "Library")
        }
    }

    enum Content {
        case candidate(BridgeCoverChoice)
        case library(BridgeGalleryCover)
    }

    let content: Content
    let label: String

    init(
        coverChoice: BridgeCoverChoice,
        label: String
    ) {
        content = .candidate(coverChoice)
        self.label = label
    }

    /// A cover of a library release's picker, worded from core's label.
    init(gallery cover: BridgeGalleryCover) {
        content = .library(cover)
        label =
            switch cover.label {
            case .current: String(localized: "Current cover")
            case .file(let name): name
            case .remote(let sourceName, let label):
                coverLabelText(sourceName: sourceName, label: label)
            }
    }
}

extension BridgeCoverChoice {
    /// The cover's image, which every slot showing it draws at its own size.
    var imageContent: ImageContent {
        ImageContent(bridge: image)
    }
}

/// Artwork lookup state rendered by the picker, with previous images retained
/// during refresh and failure so release browsing is never interrupted.
enum RemoteCoverItems: Equatable {
    case loading([CoverItem])
    case unlinked
    case linked([CoverItem])
    case failed([CoverItem], failure: DisplayError?)

    init(_ gallery: BridgeRemoteCoverGallery) {
        switch gallery {
        case .unlinked: self = .unlinked
        case .linked(let covers):
            self = .linked(
                covers.map {
                    CoverItem(coverChoice: $0.coverChoice, label: $0.labelText)
                }
            )
        }
    }

    var items: [CoverItem] {
        switch self {
        case .unlinked: []
        case .loading(let items), .linked(let items), .failed(let items, _):
            items
        }
    }

    var isLoading: Bool {
        if case .loading = self { return true }
        return false
    }

    var canRefresh: Bool {
        if case .unlinked = self { return false }
        return true
    }

    var failure: DisplayError? {
        if case .failed(_, let failure) = self { return failure }
        return nil
    }

    var hasStatus: Bool {
        switch self {
        case .loading, .unlinked: true
        case .linked(let items): items.isEmpty
        case .failed(_, let failure): failure != nil
        }
    }
}
