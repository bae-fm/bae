import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Playback pane over the Settings window: with Play on other
    /// devices off (`mac.settings.playback`), then its Play on group as the
    /// boards `mac.settings.playback.receivers-*` and `receiver-*` draw it:
    /// the receivers found, the search before the first answer, Kitchen
    /// once it has held bae's request for its owner, the ⋯ menu of the
    /// receiver playing open, and its details popover.
    static let settingsPlaybackScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.playback",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            playbackStage(
                receivers: BridgeCastReceiverList(
                    casting: nil,
                    found: [],
                    isEmpty: true
                ),
                playsOnOtherDevices: false
            ) { $0 }
        },
        ShotScene(
            id: "mac.settings.playback.receivers-found",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            playbackStage(receivers: foundReceivers) { $0 }
        },
        ShotScene(
            id: "mac.settings.playback.receivers-looking",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            playbackStage(
                receivers: BridgeCastReceiverList(
                    casting: nil,
                    found: [],
                    isEmpty: true
                )
            ) { $0 }
        },
        ShotScene(
            id: "mac.settings.playback.receivers-owner-deciding",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            playbackStage(
                receivers: foundReceivers,
                start: CastStart.drawn(
                    connecting: CastStart.Connecting(
                        receiver: foundReceivers.found[0],
                        ownerDeciding: true
                    )
                )
            ) { $0 }
        },
        ShotScene(
            id: "mac.settings.playback.receiver-menu",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            playbackStage(receivers: foundReceivers) { window in
                ZStack(alignment: .topTrailing) {
                    window
                    MacMenuInPlace {
                        MacMenuItem("Details…") {}
                        MacMenuItem("Disconnect") {}
                    }
                    .padding(.top, 346)
                    .padding(.trailing, 30)
                }
            }
        },
        ShotScene(
            id: "mac.settings.playback.receiver-details",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            playbackStage(receivers: foundReceivers) { window in
                OverWindow.popover(
                    CastReceiverDetails(receiver: livingRoom)
                        .windowBackground(),
                    top: 330,
                    trailing: 12,
                    over: window
                )
            }
        },
    ]

    /// The receiver playback is on, as the boards name it.
    private static let livingRoom = BridgeCastReceiver(
        id: "cast-living-room",
        name: "Living Room",
        kind: .cast,
        model: "Google Nest Audio",
        address: "192.168.1.42",
        takesVolume: true
    )

    /// The receivers the boards list: Living Room playing, then the rest.
    private static let foundReceivers = BridgeCastReceiverList(
        casting: livingRoom,
        found: [
            BridgeCastReceiver(
                id: "airplay-kitchen",
                name: "Kitchen",
                kind: .airPlay,
                model: "HomePod mini",
                address: "192.168.1.63",
                takesVolume: true
            ),
            BridgeCastReceiver(
                id: "upnp-den",
                name: "Den receiver",
                kind: .dlna,
                model: nil,
                address: "192.168.1.57",
                takesVolume: true
            ),
        ],
        isEmpty: false
    )

    /// The Playback pane with pausing between sides on and restoring on
    /// launch on, and Play on other devices on unless `playsOnOtherDevices`
    /// is false, as the boards draw it, with `start`'s cast connecting.
    /// `over` lays what a board adds over the window.
    private static func playbackStage<Over: View>(
        receivers: BridgeCastReceiverList,
        playsOnOtherDevices: Bool = true,
        start: CastStart = .stub(),
        over: @MainActor (AnyView) -> Over
    ) -> ShotStage {
        let castStore = CastStore()
        castStore.receivers = receivers
        if let device = receivers.casting {
            castStore.applyStatus(.on(device: device))
        }
        guard
            let defaults = UserDefaults(
                suiteName: "fm.bae.shots.settings-playback"
            )
        else {
            preconditionFailure("a named defaults suite always opens")
        }
        defaults.set(true, forKey: "persistPlayback")
        let window = SettingsWindowShot(
            tab: .playback,
            page: PlaybackSettingsTab()
                .defaultAppStorage(defaults)
                .environment(Playback.stub())
                .environment(Cast.stub())
                .environment(castStore)
                .environment(start)
                .environment(
                    PreviewData.makeConfigStore(
                        libraryFullWidth: false,
                        pauseBetweenSides: true,
                        castEnabled: playsOnOtherDevices
                    )
                )
                .environment(UiStore())
        )
        return ShotStage(over(AnyView(window)))
    }
}
