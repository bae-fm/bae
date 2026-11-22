import AppKit
import BaeKit
import CryptoKit
import Foundation

@testable import bae

/// Fixtures for the Import flow.
extension PreviewData {
    /// The watched folder the import shots read from, as text.
    static let importRootPath = "/Music/Downloads"

    /// The watched folder the import shots read from, as coven's bytes for
    /// its path.
    static let importWatchedFolderPath = Data(importRootPath.utf8)

    // MARK: - Generated placeholder art

    /// A generated stand-in cover PNG for a fixture image, written to the
    /// temporary directory under a name that holds a digest of its pixels,
    /// so a change to how the cover is drawn never reads an older file.
    static func previewArtPath(_ name: String) -> String {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("bae-preview-art", isDirectory: true)
        let data = previewArtData(name)
        let digest = SHA256.hash(data: data).prefix(8)
            .map { String(format: "%02x", $0) }
            .joined()
        let file =
            directory
            .appendingPathComponent(
                name.replacingOccurrences(of: "/", with: "-")
                    + "-\(digest)"
            )
            .appendingPathExtension("png")
        if !FileManager.default.fileExists(atPath: file.path) {
            // Crash with the reason rather than draw failure placeholders.
            // swiftlint:disable force_try
            try! FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            try! data.write(to: file)
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

    /// PNG bytes for one stand-in cover, drawn as the design system's
    /// Artwork draws it: a 135-degree fade from `hsl(hue 42% 52%)` to
    /// `hsl(hue+48 46% 24%)`, with the hue the name folds to (`hueOf`:
    /// `x = (x * 31 + code) mod 360` over its UTF-16 code units).
    static func previewArtData(_ name: String) -> Data {
        let side: CGFloat = 600
        let hue = name.utf16.reduce(0) { ($0 * 31 + Int($1)) % 360 }
        guard
            let gradient = NSGradient(
                starting: hsl(hue, saturation: 0.42, lightness: 0.52),
                ending: hsl(hue + 48, saturation: 0.46, lightness: 0.24)
            )
        else {
            preconditionFailure("a cover gradient has two colours")
        }
        let image = NSImage(
            size: NSSize(width: side, height: side),
            flipped: false
        ) { rect in
            // From the top left corner to the bottom right one.
            gradient.draw(in: rect, angle: -45)
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

    /// The colour CSS's `hsl(hue saturation lightness)` names, with the
    /// hue in degrees.
    private static func hsl(
        _ hue: Int,
        saturation: CGFloat,
        lightness: CGFloat
    ) -> NSColor {
        let brightness = lightness + saturation * min(lightness, 1 - lightness)
        return NSColor(
            hue: CGFloat(hue % 360) / 360,
            saturation: brightness == 0 ? 0 : 2 * (1 - lightness / brightness),
            brightness: brightness,
            alpha: 1
        )
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
        ),
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Live Recording 2023".utf8),
            folderPath: Data("/Music/Downloads/Live Recording 2023".utf8),
            sourceFolderName: "Live Recording 2023",
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: bridgeCandidateFiles,
            trackCount: 18,
            skipped: false,
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
        ),
    ]
    .map { Candidate(folder: $0, folders: .one(folderPath: $0.folderPath)) }

    /// Folders that failed validation, listed under Skipped: one whose
    /// tags name the release and whose own cover decodes, one with only
    /// its folder's name to go by, and one with no audio at all.
    static let invalidCandidates: [BridgeInvalidCandidate] = [
        BridgeInvalidCandidate(
            candidateKey: Data("/Music/Downloads/Half Copied".utf8),
            folderPath: Data("/Music/Downloads/Half Copied".utf8),
            sourceFolderName: "Half Copied",
            watchedFolderPath: Data("/Music/Downloads".utf8),
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
            separable: false,
            reason: .noValidAudio,
            tags: nil,
            cover: nil
        ),
    ]

    private static let releaseQueueRoot = "/Music/Incoming"

    static let releaseQueueGroupKey = BridgeFolderReleaseDecisionKey(
        watchedFolderPath: Data(releaseQueueRoot.utf8),
        relativeFolder: Data("Collection".utf8)
    )

    private static func releaseQueueRow(
        name: String,
        folder: String,
        separable: Bool
    ) -> BridgeTriageRow {
        BridgeTriageRow(
            candidateKey: Data("\(releaseQueueRoot)/\(folder)".utf8),
            folderName: name,
            watchedFolderPath: Data(releaseQueueRoot.utf8),
            actionable: true,
            placement: .pending,
            live: BridgeCandidateLiveState(
                identifying: nil,
                import: nil,
                actions: [
                    .import, .identify, .resetToFileMetadata,
                    .clearMetadata, .skip,
                ] + (separable ? [.separate] : []) + [.revealFolder],
                standing: .notLookedUp,
                badge: nil
            ),
            metadataSummary: nil,
            cover: nil,
            importStatus: nil,
            upload: nil,
            metadataProvenance: nil,
            reading: .unidentified
        )
    }

    private static let releaseQueueRows = [
        releaseQueueRow(
            name: "Release 01",
            folder: "Collection/Release 01",
            separable: false
        ),
        releaseQueueRow(
            name: "Release 02",
            folder: "Collection/Release 02",
            separable: false
        ),
        releaseQueueRow(
            name: "Release 03",
            folder: "Release 03",
            separable: false
        ),
    ]

    private static let releaseQueueCandidates = releaseQueueRows.map {
        row -> Candidate in
        var candidate = Candidate(
            folder: BridgeFolderCandidate(
                parts: [],
                candidateKey: row.candidateKey,
                folderPath: row.candidateKey,
                sourceFolderName: row.folderName,
                watchedFolderPath: Data(releaseQueueRoot.utf8),
                files: candidateFilesTracks,
                trackCount: 9,
                skipped: false,
            ),
            folders: .one(folderPath: row.candidateKey)
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
        hasWatchedFolders: true,
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
        for candidate in releaseQueueCandidates {
            store.selectedCandidates[candidate.key] = candidate
        }
        return ImportPreviewFixture(
            store: store,
            itemsByTab: [.pending: items, .done: [], .skipped: []],
            tabCounts: BridgeTriageTabCounts(
                pending: pending,
                done: 0,
                skipped: 0
            )
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

    /// Every Import-tab state in one fixture, with one release that bae is
    /// still identifying.
    @MainActor
    static func importSmokeTestScene() -> ImportPreviewFixture {
        let scene = importTabScene()
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
            itemsByTab: scene.itemsByTab,
            tabCounts: scene.tabCounts
        )
    }
}
