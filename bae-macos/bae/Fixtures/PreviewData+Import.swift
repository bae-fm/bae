import AppKit
import BaeKit
import Foundation

@testable import bae

/// Fixtures for the Import flow.
extension PreviewData {
    /// The watched folder the import shots read from, as text.
    static let importRootPath = "/Music/Downloads"

    static let importWatchedFolder = BridgeWatchedFolder(
        path: Data(importRootPath.utf8),
        pathText: importRootPath,
        name: "Downloads"
    )

    // MARK: - Generated placeholder art

    /// A generated placeholder PNG for a fixture image, written to the
    /// temporary directory on first use.
    static func previewArtPath(_ name: String) -> String {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("bae-preview-art", isDirectory: true)
        let file =
            directory
            .appendingPathComponent(
                name.replacingOccurrences(of: "/", with: "-")
            )
            .appendingPathExtension("png")
        if !FileManager.default.fileExists(atPath: file.path) {
            // Crash with the reason rather than draw failure placeholders.
            // swiftlint:disable force_try
            try! FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            try! previewArtData(name).write(to: file)
            // swiftlint:enable force_try
        }
        return file.path
    }

    /// `previewArtPath`'s file at the version on disk, as core's scan
    /// reports a folder's image.
    static func previewArtFile(_ name: String) -> BridgeFileVersion {
        let path = previewArtPath(name)
        // Crash with the reason rather than draw failure placeholders.
        // swiftlint:disable:next force_try
        let attributes = try! FileManager.default.attributesOfItem(
            atPath: path
        )
        guard let size = attributes[.size] as? UInt64,
            let modified = attributes[.modificationDate] as? Date
        else {
            preconditionFailure("placeholder art has a size and a date")
        }
        return BridgeFileVersion(
            path: URL(fileURLWithPath: path).pathBytes,
            size: size,
            modifiedAtNs: Int64(
                (modified.timeIntervalSince1970 * 1_000_000_000).rounded()
            )
        )
    }

    /// PNG bytes for one placeholder, its hue decided by the name.
    static func previewArtData(_ name: String) -> Data {
        let side: CGFloat = 600
        let hash = name.unicodeScalars.reduce(into: UInt32(5381)) {
            $0 = $0 &* 33 &+ $1.value
        }
        let fill = NSColor(
            hue: CGFloat(hash % 360) / 360,
            saturation: 0.35,
            brightness: 0.5,
            alpha: 1
        )
        let image = NSImage(
            size: NSSize(width: side, height: side),
            flipped: false
        ) { rect in
            fill.setFill()
            rect.fill()
            let text = NSAttributedString(
                string: name,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 56, weight: .semibold),
                    .foregroundColor: NSColor.white.withAlphaComponent(
                        0.85
                    ),
                ]
            )
            let textSize = text.size()
            text.draw(
                at: NSPoint(
                    x: (rect.width - textSize.width) / 2,
                    y: (rect.height - textSize.height) / 2
                )
            )
            return true
        }
        guard
            let tiff = image.tiffRepresentation,
            let bitmap = NSBitmapImageRep(data: tiff),
            let png = bitmap.representation(using: .png, properties: [:])
        else {
            preconditionFailure("placeholder art must encode as PNG")
        }
        return png
    }

    /// An image store that serves fixture "URLs", which are paths to
    /// generated placeholder art, from disk.
    static func artImageStore() -> ImageStore {
        ImageStore(
            fetchRemoteImage: { image, _ in
                try Data(contentsOf: URL(fileURLWithPath: image.url))
            }
        )
    }

    static let folderCandidates: [Candidate] = [
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Album Title One".utf8),
            folderPath: Data("/Music/Downloads/Album Title One".utf8),
            sourceFolderName: "Album Title One",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 9,
            skipped: false,
            isAdded: false
        ),
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data(
                "/Music/Downloads/Album Title Two [Label CAT-002]".utf8
            ),
            folderPath: Data(
                "/Music/Downloads/Album Title Two [Label CAT-002]".utf8
            ),
            sourceFolderName: "Album Title Two",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 12,
            // Skipped.
            skipped: true,
            isAdded: false
        ),
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Compilation Vol. 3".utf8),
            folderPath: Data("/Music/Downloads/Compilation Vol. 3".utf8),
            sourceFolderName: "Compilation Vol. 3",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 15,
            skipped: false,
            isAdded: false
        ),
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/EP Release".utf8),
            folderPath: Data("/Music/Downloads/EP Release".utf8),
            sourceFolderName: "EP Release",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 5,
            skipped: false,
            isAdded: false
        ),
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Live Recording 2023".utf8),
            folderPath: Data("/Music/Downloads/Live Recording 2023".utf8),
            sourceFolderName: "Live Recording 2023",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 18,
            // Already imported (content-hash match).
            skipped: false,
            isAdded: true
        ),
        // Two more, so Pending shows a group beside an ungrouped row.
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Album Title Three".utf8),
            folderPath: Data("/Music/Downloads/Album Title Three".utf8),
            sourceFolderName: "Album Title Three",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 11,
            skipped: false,
            isAdded: false
        ),
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Single Release".utf8),
            folderPath: Data("/Music/Downloads/Single Release".utf8),
            sourceFolderName: "Single Release",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: candidateFilesTracks,
            trackCount: 2,
            skipped: false,
            isAdded: false
        ),
    ]
    .map(Candidate.init(bridge:))

    /// Folders that failed validation, listed under Skipped: one whose
    /// tags name the release and whose own cover decodes, one with only
    /// its folder's name to go by, and one with no audio at all.
    static let invalidCandidates: [BridgeInvalidCandidate] = [
        BridgeInvalidCandidate(
            candidateKey: Data("/Music/Downloads/Half Copied".utf8),
            folderPath: Data("/Music/Downloads/Half Copied".utf8),
            sourceFolderName: "Half Copied",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            displayPath: "Half Copied",
            separable: false,
            reason: .damagedAudio(count: 2),
            tags: BridgeSkippedTags(
                albumTitle: "Album Title",
                artist: .named("Artist Name")
            ),
            cover: .local(file: previewArtFile("Front.png"))
        ),
        BridgeInvalidCandidate(
            candidateKey: Data("/Music/Downloads/Broken Rip".utf8),
            folderPath: Data("/Music/Downloads/Broken Rip".utf8),
            sourceFolderName: "Broken Rip",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            displayPath: "Broken Rip",
            separable: false,
            reason: .damagedAudio(count: 1),
            tags: nil,
            cover: nil
        ),
        BridgeInvalidCandidate(
            candidateKey: Data("/Music/Downloads/Documents Only".utf8),
            folderPath: Data("/Music/Downloads/Documents Only".utf8),
            sourceFolderName: "Documents Only",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            displayPath: "Documents Only",
            separable: false,
            reason: .noValidAudio,
            tags: nil,
            cover: nil
        ),
    ]

    private static let releaseQueueRoot = "/Music/Incoming"

    static let releaseQueueWatchedFolder = BridgeWatchedFolder(
        path: Data(releaseQueueRoot.utf8),
        pathText: releaseQueueRoot,
        name: "Incoming"
    )

    static let releaseQueueGroupKey = BridgeFolderReleaseDecisionKey(
        watchedFolderPath: Data(releaseQueueRoot.utf8),
        relativeFolder: Data("Collection".utf8)
    )

    private static func releaseQueueRow(
        name: String,
        displayPath: String,
        separable: Bool
    ) -> BridgeTriageRow {
        BridgeTriageRow(
            candidateKey: Data("\(releaseQueueRoot)/\(displayPath)".utf8),
            folderName: name,
            watchedFolderPath: Data(releaseQueueRoot.utf8),
            displayPath: displayPath,
            actionable: true,
            placement: .pending,
            live: BridgeCandidateLiveState(
                identifying: nil,
                readingPick: false,
                import: nil,
                actions: [
                    .import, .identify, .resetToFileMetadata,
                    .clearMetadata, .skip,
                    separable ? .separate : .combine, .revealFolder,
                ],
                standing: .notLookedUp,
                badge: nil
            ),
            metadataSummary: nil,
            cover: nil,
            importStatus: nil,
            upload: nil,
            metadataProvenance: nil,
            reading: .unidentified,
            selected: false
        )
    }

    private static let releaseQueueRows = [
        releaseQueueRow(
            name: "Release 01",
            displayPath: "Collection/Release 01",
            separable: false
        ),
        releaseQueueRow(
            name: "Release 02",
            displayPath: "Collection/Release 02",
            separable: false
        ),
        releaseQueueRow(
            name: "Release 03",
            displayPath: "Release 03",
            separable: false
        ),
    ]

    private static let releaseQueueCandidates = releaseQueueRows.map {
        row -> Candidate in
        var candidate = Candidate(
            bridge: BridgeFolderCandidate(
                parts: [],
                candidateKey: row.candidateKey,
                folderPath: row.candidateKey,
                sourceFolderName: row.folderName,
                watchedFolderPath: Data(releaseQueueRoot.utf8),
                files: candidateFilesTracks,
                trackCount: 9,
                skipped: false,
                isAdded: false
            )
        )
        return candidate
    }

    private static let releaseQueueGroupHeader = groupHeaderItem(
        key: releaseQueueGroupKey,
        name: "Collection",
        entryCount: 2
    )

    private static let releaseQueueItems =
        [releaseQueueGroupHeader]
        + releaseQueueRows[0...1].map(candidateItem)
        + [candidateItem(releaseQueueRows[2])]

    private static let releaseQueueSummary = importQueueSummary(
        watchedFolders: [releaseQueueWatchedFolder],
        hasGroups: true
    )

    @MainActor
    private static func releaseQueueScene(
        items: [BridgeImportListItem],
        summary: BridgeImportQueueSummary,
        pending: UInt32
    ) -> ImportPreviewFixture {
        let store = ImportStore()
        store.applySummary(summary)
        store.tabCounts = tabCounts(pending: pending, done: 0, skipped: 0)
        for candidate in releaseQueueCandidates {
            store.selectedCandidates[candidate.key] = candidate
        }
        return ImportPreviewFixture(
            store: store,
            itemsByTab: [.pending: items, .done: [], .skipped: []]
        )
    }

    @MainActor
    static func releaseQueueScene() -> ImportPreviewFixture {
        releaseQueueScene(
            items: releaseQueueItems,
            summary: releaseQueueSummary,
            pending: 3
        )
    }

    /// Every Import-tab state in one fixture, with a second watched root
    /// so the folder menu has more than one entry.
    @MainActor
    static func importSmokeTestScene() -> ImportPreviewFixture {
        let scene = importTabScene()
        guard let base = scene.store.summary else {
            preconditionFailure("the tab scene's store holds a summary")
        }
        scene.store.applySummary(
            importQueueSummary(
                watchedFolders: base.watchedFolders
                    + [releaseQueueWatchedFolder],
                hasGroups: base.hasGroups
            )
        )
        // Counted off the scene's rows, with the one still identifying
        // yet to land.
        let queue =
            importTabCounts.pending + importTabCounts.done
            + importTabCounts.skipped
        scene.store.identificationProgress = (
            identified: queue - 1, total: queue
        )
        return ImportPreviewFixture(
            store: scene.store,
            itemsByTab: scene.itemsByTab
        )
    }
}
