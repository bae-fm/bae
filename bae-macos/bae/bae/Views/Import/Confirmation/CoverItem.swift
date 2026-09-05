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
        case .releaseFile(_, let file): .releaseImage(fileId: file.id)
        case .currentCover: nil
        }
    }
    var previewContent: ImageContent {
        switch content {
        case .candidate(let choice): choice.previewContent
        case .releaseFile(let releaseId, let file):
            .releaseImage(
                releaseId: releaseId,
                source: .releaseFile(fileId: file.id)
            )
        case .currentCover(let releaseId, let image):
            .releaseImage(releaseId: releaseId, source: .cover(image: image))
        }
    }

    var thumbnailContent: ImageContent {
        switch content {
        case .candidate(let choice): choice.thumbnailContent
        case .releaseFile, .currentCover: previewContent
        }
    }

    var sourceLabel: String {
        switch selection {
        case .remoteCover(let remote):
            bridgeMetadataSourceName(source: remote.source)
        case .releaseImage: String(localized: "Release Files")
        case .embeddedCover: coreString("ui.import.metadata.file_tags")
        case nil: String(localized: "Library")
        }
    }

    enum Content {
        case candidate(BridgeCoverChoice)
        case releaseFile(releaseId: String, file: BridgeFile)
        case currentCover(releaseId: String, image: BridgeImageRef)
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

    init(releaseId: String, file: BridgeFile) {
        content = .releaseFile(releaseId: releaseId, file: file)
        label = file.originalFilename
    }

    init(releaseId: String, cover: BridgeImageRef) {
        content = .currentCover(releaseId: releaseId, image: cover)
        label = String(localized: "Current Cover")
    }
}

extension BridgeCoverChoice {
    var previewContent: ImageContent {
        ImageContent(bridge: previewSource)
    }

    var thumbnailContent: ImageContent {
        ImageContent(bridge: thumbnailSource)
    }
}

/// Artwork lookup state rendered by the picker, with previous images retained
/// during refresh and failure so release browsing is never interrupted.
enum RemoteCoverItems: Equatable {
    case loading([CoverItem])
    case unlinked
    case linked([CoverItem])
    case failed([CoverItem], message: String?)

    init(_ gallery: BridgeRemoteCoverGallery) {
        switch gallery {
        case .unlinked: self = .unlinked
        case .linked(let covers):
            self = .linked(
                covers.map {
                    CoverItem(coverChoice: $0.coverChoice, label: $0.label)
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

    var failureMessage: String? {
        if case .failed(_, let message) = self { return message }
        return nil
    }

    var hasStatus: Bool {
        switch self {
        case .loading, .unlinked: true
        case .linked(let items): items.isEmpty
        case .failed(_, let message): message != nil
        }
    }
}
