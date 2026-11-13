import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The casting boards (`mac.playback.casting*`): the library window with
    /// "Low Tide Hours" playing on a receiver.
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
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        }
    ]
}
