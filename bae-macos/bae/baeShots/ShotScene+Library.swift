import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Library views whose looks the tests leave to the gallery: the volume
    /// slider in the accent, a side whose positions run wider than a digit,
    /// a file's document panel over the window, and the add-device sheet's
    /// code and the device that joined from it.
    static let libraryScenes: [ShotScene] = [
        ShotScene(id: "volume-slider", size: CGSize(width: 152, height: 52)) {
            AnyView(
                SlimSlider(value: 0.6, onChange: { _ in })
                    .frame(width: 120)
                    .padding()
                    .background(Theme.surface)
            )
        },
        ShotScene(
            id: "album-track-list-wide-positions",
            size: CGSize(width: 800, height: 600)
        ) {
            AnyView(
                AlbumTrackListView(
                    release: widePositionsRelease(),
                    isCompilation: false,
                    currentTrackId: nil,
                    isPlaying: false,
                    onPlayFromTrack: { _ in },
                    onTogglePlayPause: {},
                    onAddNext: { _ in },
                    onAddToQueue: { _ in },
                    onExportTrack: { _ in },
                )
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .top
                )
                .padding()
                .background(Theme.background)
                .environment(UiStore())
            )
        },
        ShotScene(id: "document-viewer", size: CGSize(width: 750, height: 600))
        {
            AnyView(
                DocumentViewerView(
                    name: "rip.log",
                    text: "Extraction log",
                    onClose: {}
                )
            )
        },
        ShotScene(id: "add-device-code", size: CGSize(width: 400, height: 600))
        {
            AnyView(
                AddDeviceSheetPreview(result: addingCode)
                    .environment(AddDeviceSheetPreview.members())
            )
        },
        ShotScene(id: "add-device-added", size: CGSize(width: 400, height: 600))
        {
            AnyView(
                AddDeviceSheetPreview(result: addingCode)
                    .environment(
                        AddDeviceSheetPreview.members(
                            addedDevice: BridgeActiveDevice(
                                id: 7,
                                name: "New Phone",
                                isSelf: false,
                                canRemove: true
                            )
                        )
                    )
            )
        },
    ]

    /// The recovery code the add-device sheet shows.
    private static var addingCode: Result<BridgeAddDevice, Error> {
        AddDeviceSheetPreview.adding(
            BridgeRestoreCodeShare(
                code: PreviewData.recoveryCode,
                qrText: PreviewData.recoveryCode,
                qrCarriesS3Key: false
            )
        )
    }

    /// A ten-track side whose positions run "A1" to "A10", so its widest
    /// label is wider than a digit.
    private static func widePositionsRelease() -> ReleaseDetail {
        var release = PreviewData.releaseDetail(albumId: "a-22")
        let firstSide = release.trackGroups[0].tracks.count
        for index in release.trackGroups[0].tracks.indices {
            release.trackGroups[0].tracks[index].positionText = "A\(index + 1)"
        }
        for index in release.tracks.indices.prefix(firstSide) {
            release.tracks[index].positionText = "A\(index + 1)"
        }
        return release
    }
}
