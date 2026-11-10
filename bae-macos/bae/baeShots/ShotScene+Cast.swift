import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The cast picker with Living Room cast to, in the width of the receivers'
    /// names and with long ones wrapping, and while a cast connects.
    static let castScenes: [ShotScene] = [
        castPicker("cast-picker", receivers: castPickerReceivers()),
        castPicker(
            "cast-picker-long-name",
            receivers: castPickerReceivers(
                livingRoom: "Upstairs Living Room Sonos Arc Soundbar",
                den: "Denon AVR-X3800H Home Theater Receiver (Basement)"
            )
        ),
        ShotScene(
            id: "cast-picker-connecting",
            size: CGSize(width: 300, height: 160)
        ) {
            ShotStage(
                CastPickerPopover(
                    receivers: [
                        BridgeCastReceiver(
                            id: "speaker-1",
                            name: "Kitchen",
                            kind: .cast,
                            casting: false
                        )
                    ],
                    endedFailure: nil,
                    connectingName: "Kitchen",
                    onCast: { _ in },
                    onDisconnect: {}
                )
                .padding()
            )
        },
    ]

    /// Three receivers with Living Room cast to, first, as core lists them.
    static func castPickerReceivers(
        livingRoom: String = "Living Room",
        den: String = "Den receiver"
    ) -> [BridgeCastReceiver] {
        [
            BridgeCastReceiver(
                id: "cast-living-room",
                name: livingRoom,
                kind: .cast,
                casting: true
            ),
            BridgeCastReceiver(
                id: "cast-den",
                name: den,
                kind: .dlna,
                casting: false
            ),
            BridgeCastReceiver(
                id: "cast-kitchen",
                name: "Kitchen",
                kind: .airPlay,
                casting: false
            ),
        ]
    }

    private static func castPicker(
        _ id: String,
        receivers: [BridgeCastReceiver]
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 300, height: 260)) {
            ShotStage(
                CastPickerPopover(
                    receivers: receivers,
                    endedFailure: nil,
                    connectingName: nil,
                    onCast: { _ in },
                    onDisconnect: {}
                )
                .padding()
            )
        }
    }
}
