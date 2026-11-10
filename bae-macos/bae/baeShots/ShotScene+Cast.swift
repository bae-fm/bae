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
            id: "cast-receiver-details",
            size: CGSize(width: 320, height: 250)
        ) {
            ShotStage(
                CastReceiverDetails(
                    receiver: BridgeCastReceiver(
                        id: "cast-living-room",
                        name: "Living Room",
                        kind: .cast,
                        model: "Google Nest Audio",
                        address: "192.168.1.42"
                    )
                )
            )
        },
        ShotScene(
            id: "cast-picker-connecting",
            size: CGSize(width: 300, height: 160)
        ) {
            ShotStage(
                CastPickerPopover(
                    receivers: BridgeCastReceiverList(
                        casting: nil,
                        found: [
                            BridgeCastReceiver(
                                id: "speaker-1",
                                name: "Kitchen",
                                kind: .cast,
                                model: nil,
                                address: "192.168.1.41"
                            )
                        ]
                    ),
                    endedFailure: nil,
                    connectingName: "Kitchen",
                    onDisconnect: {}
                )
                .environment(CastStart.stub())
                .padding()
            )
        },
    ]

    /// Three receivers with Living Room cast to, as core lists them.
    static func castPickerReceivers(
        livingRoom: String = "Living Room",
        den: String = "Den receiver"
    ) -> BridgeCastReceiverList {
        BridgeCastReceiverList(
            casting: BridgeCastReceiver(
                id: "cast-living-room",
                name: livingRoom,
                kind: .cast,
                model: "Google Nest Audio",
                address: "192.168.1.42"
            ),
            found: [
                BridgeCastReceiver(
                    id: "cast-den",
                    name: den,
                    kind: .dlna,
                    model: nil,
                    address: "192.168.1.57"
                ),
                BridgeCastReceiver(
                    id: "cast-kitchen",
                    name: "Kitchen",
                    kind: .airPlay,
                    model: "HomePod mini",
                    address: "192.168.1.63"
                ),
            ]
        )
    }

    private static func castPicker(
        _ id: String,
        receivers: BridgeCastReceiverList
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 300, height: 260)) {
            ShotStage(
                CastPickerPopover(
                    receivers: receivers,
                    endedFailure: nil,
                    connectingName: nil,
                    onDisconnect: {}
                )
                .environment(CastStart.stub())
                .padding()
            )
        }
    }
}
