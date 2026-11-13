import BaeKit
import Foundation
import SwiftUI

@testable import bae

/// The named UI scenes captured for the cross-platform screenshot gallery.
/// Each builder composes production views over `PreviewData` fixtures.
@MainActor
enum ShotScenes {
    /// The welcome window entered at `initialMode`, in its fixed 690x600
    /// chrome, over `setup` and the `LibraryRemovals` fixtures. Every flow
    /// can go back to the chooser, which reads both lists from `setup`.
    static func welcome(
        initialMode: WelcomeView.Mode,
        setup: LibrarySetup,
        removals: LibraryRemovals = PreviewData.welcomeRemovals(),
        loadError: LoadError? = nil
    ) -> some View {
        MacWelcomeWindow {
            WelcomeView(
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                onFinishReplacement: { _, _ in },
                onResetFromCloud: { _ in },
                loadError: loadError,
                initialMode: initialMode,
                canDeleteActiveLibrary: true
            )
        }
        .environment(setup)
        .environment(removals)
    }

    /// What the details popover of `library` holds, its status read
    /// through `readStanding`.
    static func welcomeLibraryDetails(
        _ library: BridgeLibrary,
        readStanding:
            @escaping @Sendable (String) async throws
            -> BridgeLibraryStanding
    ) -> some View {
        WelcomeLibraryDetails(
            library: library,
            readStanding: readStanding
        )
        .windowBackground()
    }

    /// The screen that offers resetting this device, in the welcome
    /// window's chrome, saying `error`, or with none, the reset's own line.
    static func deviceReset(error: LoadError?) -> some View {
        MacWelcomeWindow {
            DeviceResetView(
                prompt: .offered,
                error: error,
                onReset: {},
                onCancel: {}
            )
        }
    }

    /// Where the now-playing bar's track is, which parts of it are here,
    /// and whether its load waits on storage.
    enum NowPlayingLoad {
        /// The load waits on storage for its first sound, with the start
        /// of the track here.
        case waitingOnStorage
        /// Playing, with a gap a seek ahead left in what is here.
        case playingWithAGap
        /// Playing, with every byte of the track here.
        case allHere

        var progress: Double {
            switch self {
            case .waitingOnStorage: 0
            case .playingWithAGap: 0.3
            case .allHere: 0.55
            }
        }

        var held: [BridgeTrackSpan] {
            switch self {
            case .waitingOnStorage:
                [BridgeTrackSpan(start: 0, end: 0.08)]
            case .playingWithAGap:
                [
                    BridgeTrackSpan(start: 0, end: 0.45),
                    BridgeTrackSpan(start: 0.62, end: 0.85),
                ]
            case .allHere:
                [BridgeTrackSpan(start: 0, end: 1)]
            }
        }

        var waitsOnStorage: Bool { self == .waitingOnStorage }
    }

    /// The now-playing bar on a track in `load`.
    static func nowPlayingBar(_ load: NowPlayingLoad) -> some View {
        NowPlayingBarPreview(
            trackTitle: PreviewData.nowPlayingTitle,
            artistNames: PreviewData.nowPlayingArtist,
            isPlaying: true,
            held: load.held,
            waitsOnStorage: load.waitsOnStorage,
            repeatMode: .off,
            shuffled: false,
            progress: load.progress
        )
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 2))
        .environment(Queue.stub())
        .windowBackground()
    }

    /// The expanded album detail for a two-sided vinyl fixture, mid-playback.
    static func albumDetail() -> some View {
        PreviewData.albumExpansionScene(
            albumId: "a-21",
            currentTrackId: "t-d2-3",
            isPlaying: true
        )
    }

    /// The padding around the rows of a `triageRows` scene.
    static let triageRowsPadding: CGFloat = 16

    private static func triageRows(
        _ rows: [BridgeTriageRow],
        images: ImageStore
    ) -> some View {
        let importStore = ImportStore()
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(rows, id: \.candidateKey) { row in
                MacImportRow(
                    row: row,
                    coverContent: importStore.sidebarCover(for: row),
                    isGroupMember: false,
                    onShowInLibrary: {}
                )
                .frame(height: ImportCandidateTableLayout.rowHeight)
            }
        }
        .padding(triageRowsPadding)
        .frame(width: 360)
        .environment(PreviewData.doneRowOutboxStore())
        .environment(images)
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }
}

extension ShotScenes {
    /// The first screen's list of this Mac's libraries, one row for each
    /// state a library is in: one with a cloud home, one with none, which
    /// offers Delete in place of Remove, one that can't be read, and one
    /// whose restore, and one whose removal, stopped partway. With
    /// `removing`, the first of `libraries` is being removed, at that step.
    static func welcomeLibraries(
        _ libraries: [BridgeLibrary] = PreviewData.welcomeLibraries,
        removing step: BridgeClosingProgress? = nil
    ) -> some View {
        LocalLibrariesSection(
            libraries: libraries,
            disabled: false,
            canDeleteActiveLibrary: true,
            removing: step.map {
                RunningRemoval(
                    libraryId: libraries[0].id,
                    steps: LibraryClosingSteps(
                        host: BaeHost.inTemporaryFolder(),
                        at: $0
                    )
                )
            },
            opening: nil,
            onDiscardInstallation: { _ in },
            onResetFromCloud: { _ in },
            onOpen: { _ in },
            onFinish: { _, _ in },
            onFinishRemoving: { _ in },
            onRemove: { _ in },
            readStanding: { _ in PreviewData.standingSynced },
            onCreate: {},
            onScanCode: {},
            onEnterCode: {}
        )
        .padding()
        .windowBackground()
    }
}
