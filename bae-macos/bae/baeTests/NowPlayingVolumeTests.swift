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
            bar(receiverWithoutVolume: "Den", settings: settings).volumeControls
                == .setOnReceiver("Den")
        )
    }

    @Test("other playback keeps the mute button and the slider")
    func muteAndSliderKept() {
        #expect(
            bar(receiverWithoutVolume: nil, settings: settings).volumeControls
                == .controls(settings)
        )
        #expect(
            bar(receiverWithoutVolume: nil, settings: nil).volumeControls
                == .waitingForCore
        )
    }

    private let settings = PlaybackSettings(
        volume: 0.7,
        isMuted: false,
        repeatMode: .off
    )

    private func bar(
        receiverWithoutVolume: String?,
        settings: PlaybackSettings?
    ) -> MacNowPlayingBar {
        MacNowPlayingBar(
            trackTitle: "Track Title",
            secondaryLine: "Artist Name",
            cover: nil,
            isPlaying: true,
            position: nil,
            buffered: nil,
            storageWait: nil,
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
            castControl: AnyView(EmptyView())
        )
    }
}
