import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Views in a wait that names itself in words beside its spinner: a
    /// catalog still searching, a cast connecting, a Discogs key being
    /// checked, the lightbox's image loading.
    static let waitScenes: [ShotScene] = [
        ShotScene(
            id: "import-search-searching",
            size: CGSize(width: 800, height: 520)
        ) {
            AnyView(
                FindOnlineSearchResults(
                    search: PreviewData.searchRunInFlight,
                    isImporting: false,
                    libraryStatuses: [:],
                    selectedReleaseId: nil,
                    loadingPick: nil,
                    onRetry: {},
                    onSelect: { _ in },
                )
                .importPreviewEnvironment()
            )
        },
        ShotScene(
            id: "cast-picker-connecting",
            size: CGSize(width: 300, height: 160)
        ) {
            AnyView(
                CastPickerPopover(
                    devices: [
                        BridgeCastDevice(
                            id: "speaker-1",
                            name: "Kitchen",
                            kind: .cast
                        )
                    ],
                    castingDevice: nil,
                    endedFailure: nil,
                    connectingName: "Kitchen",
                    onCast: { _ in },
                    onDisconnect: {}
                )
                .padding()
            )
        },
        ShotScene(
            id: "settings-discogs-checking",
            size: CGSize(width: 500, height: 320)
        ) {
            AnyView(
                DiscogsKeyPreview(
                    status: .unvalidated,
                    draft: "abcdef123456",
                    isValidating: true
                )
            )
        },
        ShotScene(
            id: "artwork-lightbox-loading",
            size: CGSize(width: 640, height: 480)
        ) {
            AnyView(LightboxView<LightboxItem>.loadingPreview())
        },
    ]
}
