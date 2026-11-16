import BaeKit
import Foundation

/// A cover the picker shows, as core's gallery read returns it, with the text
/// that names it: what the lightbox previews and what saving it selects.
struct CoverItem: LightboxImage {
    static func == (lhs: CoverItem, rhs: CoverItem) -> Bool {
        lhs.id == rhs.id
    }

    var id: BridgeCoverKey { cover.key }

    var selection: BridgeCoverSelection? { cover.selection }
    var image: ImageContent { ImageContent(bridge: cover.image) }

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

    let cover: BridgeGalleryCover
    let label: String

    /// Makes an item for a cover in a picker, labeled with the text for
    /// core's label.
    init(gallery cover: BridgeGalleryCover) {
        self.cover = cover
        label =
            switch cover.label {
            case .current: String(localized: "Cover")
            case .file(let name, _): name
            case .remote(let sourceName, let label, _):
                coverLabelText(sourceName: sourceName, label: label)
            }
    }
}

extension BridgeGalleryCover {
    /// What saving the cover selects, which its label holds: none for the
    /// cover the release has now.
    var selection: BridgeCoverSelection? {
        switch label {
        case .current: nil
        case .file(_, let selection), .remote(_, _, let selection): selection
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
