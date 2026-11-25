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
        ShotScene(
            id: "mac.import.folder.found",
            size: CGSize(width: 1_400, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: nil),
                size: CGSize(width: 1_400, height: 900)
            )
        },
        ShotScene(
            id: "mac.import.folder.identified",
            size: CGSize(width: 1_400, height: 1_100),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .identified, opening: .northbound),
                size: CGSize(width: 1_400, height: 1_100)
            )
        },
        ShotScene(
            id: "mac.import.folder.importing",
            size: CGSize(width: 1_400, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: .northbound),
                size: CGSize(width: 1_400, height: 900)
            )
        },
        ShotScene(
            id: "mac.import.folder.already-in-library",
            size: CGSize(width: 1_400, height: 1_100),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: .lowTideHours),
                size: CGSize(width: 1_400, height: 1_100)
            )
        },
        ShotScene(
            id: "mac.import.folder.nothing-read",
            size: CGSize(width: 1_400, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: .untitled07),
                size: CGSize(width: 1_400, height: 900)
            )
        },
        ShotScene(
            id: "mac.import.folder.cue-sheets",
            size: CGSize(width: 1_400, height: 1_100),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: .fieldRecordings),
                size: CGSize(width: 1_400, height: 1_100)
            )
        },
        ShotScene(
            id: "mac.import.folder.imported",
            size: CGSize(width: 1_400, height: 1_100),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .imported,
                size: CGSize(width: 1_400, height: 1_100),
                releaseEditor: PreviewData.releaseEditor(
                    holding: PreviewData.northboundRelease(
                        cover: cover(PreviewData.northboundTitle)
                    )
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
        ShotScene(
            id: "mac.import.narrow.folder",
            size: CGSize(width: 900, height: 1_100),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .identified, opening: .northbound),
                size: CGSize(width: 900, height: 1_100)
            )
        },
        ShotScene(
            id: "mac.import.narrow.several",
            size: CGSize(width: 900, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .several,
                size: CGSize(width: 900, height: 900)
            )
        },
        ShotScene(
            id: "mac.import.folder.skipped",
            size: CGSize(width: 1_400, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .skipped,
                size: CGSize(width: 1_400, height: 900)
            )
        },
        ShotScene(
            id: "mac.import.identify.search",
            size: CGSize(width: 1_400, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: .copperSky),
                size: CGSize(width: 1_400, height: 900)
            )
        },
        ShotScene(
            id: "mac.import.identify.no-discogs-key",
            size: CGSize(width: 1_400, height: 900),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            try importPaneStage(
                .found(northbound: .importing, opening: .winterKitchen),
                size: CGSize(width: 1_400, height: 900),
                discogsUsable: false
            )
        },
    ]

    /// The main window on Import as `board` draws it, in a library that
    /// can ask Discogs when `discogsUsable`.
    @MainActor
    private static func importPaneStage(
        _ board: PreviewData.ImportBoard,
        size: CGSize,
        discogsUsable: Bool = true,
        releaseEditor: ReleaseEditor? = nil,
        isDrawn: @escaping @MainActor (NSView) -> Bool = { _ in true }
    ) throws -> ShotStage {
        let images = PreviewData.artImageStore()
        return ShotStage(
            try importWindow(
                board,
                uiStore: UiStore(),
                images: images,
                size: size,
                discogsUsable: discogsUsable,
                releaseEditor: releaseEditor
            ),
            images: images,
            isDrawn: isDrawn
        )
    }

    /// `window` on Import as `board` draws it, over `uiStore`, with its art
    /// from `images`.
    @MainActor
    static func importWindow(
        _ board: PreviewData.ImportBoard,
        uiStore: UiStore,
        images: ImageStore,
        size: CGSize,
        discogsUsable: Bool = true,
        releaseEditor: ReleaseEditor? = nil,
        window: AnyView = AnyView(MacMainWindowContainer())
    ) throws -> some View {
        uiStore.activeSection = .importing
        uiStore.setImportCandidateTab(board.tab)
        let scene = try PreviewData.importBoardScene(board)
        return ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: boardLibraryBacking(uiStore: uiStore),
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
            size: size,
            window: window
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
