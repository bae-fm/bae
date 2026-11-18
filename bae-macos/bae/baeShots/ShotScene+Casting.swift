import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The casting boards (`mac.playback.casting*`): the library window with
    /// "Low Tide Hours" playing on a receiver; and the pairing boards
    /// (`mac.playback.cast-pin*`): the sheet that asks for an AirPlay
    /// receiver's code, and the alerts for a cast that failed, over the
    /// library window.
    static let castingScenes: [ShotScene] = [
        ShotScene(
            id: "mac.playback.casting.no-volume",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore)
            let castStore = CastStore()
            // A receiver that takes no volume from bae, as a UPnP renderer
            // with no RenderingControl service is.
            castStore.applyStatus(
                .on(
                    device: BridgeCastReceiver(
                        id: "living-room",
                        name: "Living Room",
                        kind: .dlna,
                        model: nil,
                        address: "192.168.1.41",
                        takesVolume: false
                    )
                )
            )
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    needsAttention: false,
                    playbackStore: boardPlayback(),
                    castStore: castStore,
                    size: CGSize(width: 1200, height: 760)
                )
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        },
        ShotScene(
            id: "mac.playback.cast-pin",
            size: CGSize(width: 1200, height: 760)
        ) {
            castPinSheet(asking: pinRequest(), typed: "48")
        },
        ShotScene(
            id: "mac.playback.cast-pin.pairing",
            size: CGSize(width: 1200, height: 760)
        ) {
            castPinSheet(asking: pinRequest(), typed: "4821", pairing: true)
        },
        ShotScene(
            id: "mac.playback.cast-pin.wrong-code",
            size: CGSize(width: 1200, height: 760)
        ) {
            castPinSheet(
                asking: pinRequest(lastTry: .wrongCode),
                typed: "4821"
            )
        },
        ShotScene(
            id: "mac.playback.cast-pin.password",
            size: CGSize(width: 1200, height: 760)
        ) {
            castPinSheet(
                asking: BridgeAirPlayCodeRequest(
                    code: .password,
                    why: .firstPairing,
                    lastTry: nil
                ),
                typed: "gardenwall"
            )
        },
        ShotScene(
            id: "mac.playback.cast-pin.refused",
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            castAlert(
                CastAlertContent.refused(
                    by: kitchen.name,
                    error: DisplayError(
                        line:
                            BridgeErrorCategory.airPlayLockedOut(
                                receiver: kitchen.name
                            )
                            .localizedLine
                    )
                )
            )
        },
        ShotScene(
            id: "mac.playback.cast-pin.pairing-lost",
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            castAlert(CastAlertContent.pairingLost(error: pairingLost) {})
        },
    ]

    /// The AirPlay receiver the pairing boards cast to.
    private static let kitchen = BridgeCastReceiver(
        id: "5855CA1A2B3C",
        name: "Kitchen",
        kind: .airPlay,
        model: nil,
        address: "192.168.1.40",
        takesVolume: true
    )

    /// The playback error for Kitchen no longer accepting this device.
    private static var pairingLost: DisplayError {
        let reason = BridgePlaybackErrorReason.audioOutput(
            problem: .airPlayPairingLost(receiver: kitchen)
        )
        guard let error = DisplayError(reason) else {
            preconditionFailure("core gives a lost pairing a line")
        }
        return error
    }

    /// A first pairing with a receiver that shows a 4-digit PIN.
    private static func pinRequest(
        lastTry: BridgeAirPlayCodeRefused? = nil
    ) -> BridgeAirPlayCodeRequest {
        BridgeAirPlayCodeRequest(
            code: .pin(digits: 4),
            why: .firstPairing,
            lastTry: lastTry
        )
    }

    /// The pairing sheet asking `request` of Kitchen over the library
    /// window, its field holding `typed`, with Pairing… once `pairing`.
    private static func castPinSheet(
        asking request: BridgeAirPlayCodeRequest,
        typed: String,
        pairing: Bool = false
    ) -> ShotStage {
        let start = CastStart.asking(request, of: kitchen, pairing: pairing)
        guard let sheet = start.codeSheet else {
            preconditionFailure("a start that asks opens its sheet")
        }
        return libraryWindowStage { window in
            AnyView(
                OverWindow.sheet(
                    CastCodeSheet(
                        sheet: sheet,
                        typed: typed,
                        onPair: { _ in },
                        onCancel: {}
                    ),
                    over: window
                )
            )
        }
    }

    /// A cast's failure alert over the library window.
    private static func castAlert(
        _ alert: (MacAlertWords, [MacAlertButton])
    ) -> ShotStage {
        libraryWindowStage { window in
            AnyView(OverWindow.alert(alert, over: window))
        }
    }

    /// The library window with "Low Tide Hours" playing, `dressed` with what
    /// opens over it.
    private static func libraryWindowStage(
        dressed: (AnyView) -> AnyView
    ) -> ShotStage {
        let uiStore = UiStore()
        let backing = boardLibraryBacking(uiStore: uiStore)
        let window = ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: backing,
            needsAttention: false,
            playbackStore: boardPlayback(),
            size: CGSize(width: 1200, height: 760)
        )
        return ShotStage(dressed(AnyView(window))) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        }
    }
}
