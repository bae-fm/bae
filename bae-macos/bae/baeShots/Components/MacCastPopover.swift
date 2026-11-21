import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacCastPopover`: over a receiver cast to, with long names, with
    /// nothing cast to, while a cast connects, while the receiver's owner
    /// decides, and with no receivers.
    static let macCastPopoverScenes: [ShotScene] = [
        castPopover(
            "mac.component.cast-popover.casting",
            casting: castReceiver("Living Room", .cast),
            found: [
                castReceiver("Kitchen", .airPlay),
                castReceiver("Den receiver", .dlna),
            ]
        ),
        castPopover(
            "mac.component.cast-popover.long-names",
            casting: castReceiver("Upstairs Living Room Soundbar", .cast),
            found: [
                castReceiver("Kitchen", .airPlay),
                castReceiver(
                    "Home theater receiver in the basement",
                    .dlna
                ),
            ]
        ),
        castPopover(
            "mac.component.cast-popover.not-casting",
            found: [
                castReceiver("Living Room", .cast),
                castReceiver("Kitchen", .airPlay),
            ]
        ),
        castPopover(
            "mac.component.cast-popover.connecting",
            found: [
                castReceiver("Living Room", .cast),
                castReceiver("Kitchen", .airPlay),
            ],
            connecting: CastStart.Connecting(
                receiver: castReceiver("Kitchen", .airPlay),
                ownerDeciding: false
            )
        ),
        castPopover(
            "mac.component.cast-popover.owner-deciding",
            found: [
                castReceiver("Living Room", .cast),
                castReceiver("Kitchen", .airPlay),
            ],
            connecting: CastStart.Connecting(
                receiver: castReceiver("Kitchen", .airPlay),
                ownerDeciding: true
            )
        ),
        castPopover("mac.component.cast-popover.no-receivers"),
    ]

    private static func castPopover(
        _ id: String,
        casting: BridgeCastReceiver? = nil,
        found: [BridgeCastReceiver] = [],
        connecting: CastStart.Connecting? = nil
    ) -> ShotScene {
        component(id, width: 284, height: 280) {
            MacCastPopover(
                receivers: BridgeCastReceiverList(
                    casting: casting,
                    found: found,
                    isEmpty: casting == nil && found.isEmpty
                ),
                endedFailure: nil,
                onDisconnect: {}
            )
            .environment(
                connecting.map { CastStart.drawn(connecting: $0) }
                    ?? CastStart.stub()
            )
        }
    }
}
