import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

/// The bar's volume controls: mute and the slider, or for a receiver that
/// takes no volume from bae, a line saying where the volume is set.
@MainActor
@Suite("Now-playing volume")
struct NowPlayingVolumeTests {
    private func receiver(takesVolume: Bool) -> BridgeCastReceiver {
        BridgeCastReceiver(
            id: "den",
            name: "Den",
            kind: .dlna,
            model: nil,
            address: "192.168.1.57",
            takesVolume: takesVolume
        )
    }

    @Test("the cast store names a receiver only when it takes no volume")
    func storeNamesTheReceiverWithoutVolume() {
        let store = CastStore()
        #expect(store.receiverWithoutVolume == nil)
        store.applyStatus(.on(device: receiver(takesVolume: true)))
        #expect(store.receiverWithoutVolume == nil)
        store.applyStatus(.on(device: receiver(takesVolume: false)))
        #expect(store.receiverWithoutVolume == "Den")
        store.applyStatus(.local)
        #expect(store.receiverWithoutVolume == nil)
    }

    @Test("a receiver without volume gets the line in place of the controls")
    func lineInPlaceOfMute() {
        #expect(
            NowPlayingBarFixture.make(
                settings: settings,
                receiverWithoutVolume: "Den"
            )
            .volumeControls
                == .setOnReceiver("Den")
        )
    }

    @Test("other playback keeps the mute button and the slider")
    func muteAndSliderKept() {
        #expect(
            NowPlayingBarFixture.make(settings: settings).volumeControls
                == .controls(settings)
        )
        #expect(
            NowPlayingBarFixture.make(settings: nil).volumeControls
                == .waitingForCore
        )
    }

    private let settings = PlaybackSettings(
        volume: 0.7,
        isMuted: false,
        repeatMode: .off
    )

}

/// A playing track in the real bar, with local fixture streams and no commands
/// that can start playback or reach a receiver.
@MainActor
enum NowPlayingBarFixture {
    struct Track {
        let position: LatestSource<BridgeTrackPosition>
        var held: [BridgeTrackSpan] = []
        var waitsOnStorage = false
    }

    static func make(
        settings: PlaybackSettings?,
        receiverWithoutVolume: String? = nil,
        castControl: AnyView = AnyView(EmptyView()),
        track: Track = Track(
            position: PreviewData.trackPosition(
                positionMs: 102_000,
                durationMs: 303_000
            )
        )
    ) -> MacNowPlayingBar {
        MacNowPlayingBar(
            trackTitle: "Track Title",
            secondaryLine: "Artist Name",
            cover: nil,
            isPlaying: true,
            position: track.position,
            buffered: PreviewData.trackBuffered(track.held),
            waitsOnStorage: track.waitsOnStorage,
            showRemainingTime: false,
            settings: settings,
            receiverWithoutVolume: receiverWithoutVolume,
            shuffled: false,
            showQueue: false,
            onPlayPause: {},
            onNext: {},
            onPrevious: {},
            onSeek: { _ in },
            onToggleRemainingTime: {},
            onVolumeDrag: { _ in },
            onVolumeChange: { _ in },
            onSetMuted: { _ in },
            onSetShuffle: { _ in },
            onSetQueuePresented: { _ in },
            onSetRepeatMode: { _ in },
            onDropToQueue: { _ in },
            onNavigateToAlbum: {},
            queueAddPublisher: Empty().eraseToAnyPublisher(),
            castControl: castControl
        )
    }
}
