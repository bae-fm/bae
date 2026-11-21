import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacCastRow` in the cast popover.
    static let macCastRowScenes: [ShotScene] = [
        component("mac.component.cast-row.receivers", height: 270) {
            ShotBox {
                MacCastRow(
                    receiver: castReceiver("Living Room", .cast),
                    role: .casting(onDisconnect: {}),
                    place: .picker,
                    wait: nil
                )
                MacCastRow(
                    receiver: castReceiver("Kitchen", .airPlay),
                    role: .found(onCast: {}),
                    place: .picker,
                    wait: nil
                )
                MacCastRow(
                    receiver: castReceiver("Den receiver", .dlna),
                    role: .found(onCast: {}),
                    place: .picker,
                    wait: nil
                )
            }
        },
        castRow("mac.component.cast-row.connecting", wait: .connecting),
        castRow("mac.component.cast-row.owner-deciding", wait: .ownerDeciding),
        castRow("mac.component.cast-row.menu", place: .page, wait: nil),
        component("mac.component.cast-row.long-name", height: 140) {
            ShotBox {
                MacCastRow(
                    receiver: castReceiver(
                        "Denon AVR-X3800H Home Theater Receiver (Basement)",
                        .dlna
                    ),
                    role: .found(onCast: {}),
                    place: .picker,
                    wait: nil
                )
            }
        },
    ]

    /// `MacCastRow` in a box, as its preview draws it: Kitchen while bae
    /// connects to it, or waits for its owner, or with the Playback settings'
    /// menu.
    private static func castRow(
        _ id: String,
        place: MacCastRow.Place = .picker,
        wait: MacCastRow.Wait?
    ) -> ShotScene {
        component(id, height: 100) {
            ShotBox {
                MacCastRow(
                    receiver: castReceiver("Kitchen", .airPlay),
                    role: .found(onCast: {}),
                    place: place,
                    wait: wait
                )
            }
        }
    }
}
