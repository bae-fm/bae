import BaeKit
import Foundation

/// A selectable candidate image or a persisted release file, retaining the
/// source's identity for both previewing and saving.
struct CoverItem: Identifiable, Equatable {
    static func == (lhs: CoverItem, rhs: CoverItem) -> Bool {
        lhs.id == rhs.id
    }

    var id: BridgeCoverSelection { selection }
    var selection: BridgeCoverSelection {
        switch content {
        case .candidate(let choice): choice.selection
        case .releaseFile(_, let file): .releaseImage(fileId: file.id)
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
        }
    }

    var thumbnailContent: ImageContent {
        switch content {
        case .candidate(let choice): choice.thumbnailContent
        case .releaseFile: previewContent
        }
    }

    var sourceLabel: String {
        switch selection {
        case .remoteCover(let remote):
            bridgeMetadataSourceName(source: remote.source)
        case .releaseImage: String(localized: "Release Files")
        case .embeddedCover: coreString("ui.import.metadata.file_tags")
        }
    }

    enum Content {
        case candidate(BridgeCoverChoice)
        case releaseFile(releaseId: String, file: BridgeFile)
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
}

extension BridgeCoverChoice {
    var previewContent: ImageContent {
        ImageContent(bridge: previewSource)
    }

    var thumbnailContent: ImageContent {
        ImageContent(bridge: thumbnailSource)
    }
}
