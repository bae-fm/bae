import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The import pane's boards, each the main window on Import as a board
    /// draws it: Found with nothing opened, or with Northbound on its draft
    /// or its running import, a release the library already holds, a folder
    /// with nothing read, or CUE sheets; Imported, uploading; Skipped; and
    /// Find online on a folder no catalog has, on Search with a typed search
    /// out, or on Automatic in a library with no Discogs key. The
    /// lists, names, progress and folders are the boards' own
    /// (`PreviewData.importBoardScene`), in the states the app reads for
    /// them.
    static let importPaneScenes: [ShotScene] = [
        importPaneScene(
            "mac.import.folder.found",
            height: 900,
            .found(northbound: .importing, opening: nil)
        ),
        importPaneScene(
            "mac.import.folder.identified",
            height: 1_260,
            .found(northbound: .identified, opening: .northbound)
        ),
        importPaneScene(
            "mac.import.folder.importing",
            height: 900,
            .found(northbound: .importing, opening: .northbound)
        ),
        importPaneScene(
            "mac.import.folder.already-in-library",
            height: 900,
            .found(northbound: .importing, opening: .lowTideHours)
        ),
        importPaneScene(
            "mac.import.folder.nothing-read",
            height: 900,
            .found(northbound: .importing, opening: .untitled07)
        ),
        importPaneScene(
            "mac.import.folder.cue-sheets",
            height: 900,
            .found(northbound: .importing, opening: .fieldRecordings)
        ),
        ShotScene(
            id: "mac.import.folder.imported",
            size: CGSize(width: 1_400, height: 1_300)
        ) {
            importPaneStage(
                .imported,
                size: CGSize(width: 1_400, height: 1_300),
                releaseEditor: PreviewData.releaseEditor(
                    holding: PreviewData.northboundReleaseDraft()
                ),
                // The editor opens the release after the pane first draws:
                // it is there once its last track's title field holds the
                // release's title for it.
                isDrawn: { host in
                    textFields(below: host)
                        .contains {
                            $0.stringValue
                                == PreviewData.northboundTracks.last?.title
                        }
                }
            )
        },
        importPaneScene("mac.import.folder.skipped", height: 900, .skipped),
        importPaneScene(
            "mac.import.identify.search",
            height: 900,
            .found(northbound: .importing, opening: .copperSky)
        ),
        ShotScene(
            id: "mac.import.identify.no-discogs-key",
            size: CGSize(width: 1_400, height: 900)
        ) {
            importPaneStage(
                .found(northbound: .importing, opening: .winterKitchen),
                size: CGSize(width: 1_400, height: 900),
                discogsUsable: false
            )
        },
    ]

    /// A board shot of `board` with nothing else to state.
    private static func importPaneScene(
        _ id: String,
        height: CGFloat,
        _ board: PreviewData.ImportBoard
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 1_400, height: height)) {
            importPaneStage(
                board,
                size: CGSize(width: 1_400, height: height)
            )
        }
    }

    /// The main window on Import as `board` draws it, in a library that
    /// can ask Discogs when `discogsUsable`.
    @MainActor
    private static func importPaneStage(
        _ board: PreviewData.ImportBoard,
        size: CGSize,
        discogsUsable: Bool = true,
        releaseEditor: ReleaseEditor? = nil,
        isDrawn: @escaping @MainActor (NSView) -> Bool = { _ in true }
    ) -> ShotStage {
        let uiStore = UiStore()
        uiStore.activeSection = .importing
        uiStore.setImportCandidateTab(board.tab)
        let scene = PreviewData.importBoardScene(board)
        let images = PreviewData.artImageStore()
        return ShotStage(
            ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: boardLibraryBacking(uiStore: uiStore),
                // The boards draw no sync activity, so nothing moves and
                // the bar shows no Activity button.
                syncActivity: SyncActivityStore(),
                playbackStore: boardPlayback(),
                importScene: scene.fixture,
                importSelection: scene.selection,
                candidateRuntimes: scene.runtimes,
                images: images,
                // The boards' library syncs, so the pane offers where an
                // import goes.
                configStore: PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    castEnabled: true,
                    discogsUsable: discogsUsable,
                    storage: PreviewData.previewStorage
                ),
                releaseEditor: releaseEditor,
                outbox: scene.outbox,
                importPercents: scene.importPercents,
                size: size
            ),
            images: images,
            isDrawn: isDrawn
        )
    }

    /// Every text field in the tree below `view`.
    @MainActor
    private static func textFields(below view: NSView) -> [NSTextField] {
        view.subviews.flatMap { subview in
            let own = (subview as? NSTextField).map { [$0] } ?? []
            return own + textFields(below: subview)
        }
    }
}
