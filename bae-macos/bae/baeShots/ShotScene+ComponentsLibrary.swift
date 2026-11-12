import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the library and the storage inspector: the
    /// headings, the album tiles, a file's row and details, and the storage
    /// chips.
    static let libraryComponentScenes: [ShotScene] =
        headingScenes + albumTileScenes + fileScenes + storageChipScenes

    /// `SectionHeader` and `Eyebrow`.
    private static let headingScenes: [ShotScene] = [
        component("mac.component.section-header.default", height: 70) {
            SectionHeader(title: "Works").frame(width: 380)
        },
        component("mac.component.eyebrow.default", height: 50) {
            Eyebrow(verbatim: "Side A")
        },
    ]

    /// `MacAlbumTile` in a row of four, the first at rest, the third open,
    /// the fourth selected; and `ArtistAlbumCard`.
    private static let albumTileScenes: [ShotScene] = [
        component("mac.component.album-tile.default", width: 720, height: 270) {
            let menu = AlbumCardMenu(
                targetCount: 1,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
                onDelete: {}
            )
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                albumTile("Low Tide Hours", "Mara Quill", 1992, menu: menu)
                albumTile(
                    "Paper Lanterns",
                    "The Slow Orchards",
                    1971,
                    menu: menu
                )
                albumTile(
                    "Glass Harbor",
                    "Ines Varga Trio",
                    1959,
                    open: true,
                    menu: menu
                )
                albumTile(
                    "Copper Sky",
                    "Odile Fen",
                    nil,
                    selected: true,
                    menu: menu
                )
            }
            .environment(ImageStore.stub())
        },
        component(
            "mac.component.artist-album-tile.default",
            width: 540,
            height: 250
        ) {
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                ForEach(0..<3, id: \.self) { index in
                    ArtistAlbumCard(
                        album: PreviewData.albums[index],
                        onTap: {}
                    )
                    .frame(width: 160)
                }
            }
            .environment(ImageStore.stub())
        },
    ]

    private static func albumTile(
        _ title: String,
        _ artist: String,
        _ year: Int32?,
        open: Bool = false,
        selected: Bool = false,
        menu: AlbumCardMenu
    ) -> some View {
        MacAlbumTile(
            title: title,
            artistNames: artist,
            year: year,
            cover: nil,
            isExpanded: open,
            isSelected: selected,
            size: 160,
            menu: menu
        )
    }

    private static func inspectorFile(
        _ name: String,
        size: UInt64,
        kind: BridgeFileKind,
        upload: BridgeFileUpload? = nil
    ) -> BridgeStorageInspectorFile {
        BridgeStorageInspectorFile(
            file: BridgeFile(
                id: name,
                originalFilename: name,
                name: name,
                folder: nil,
                kind: kind,
                fileSize: size,
                contentType: "application/octet-stream",
                isImage: false,
                audioFormat: nil
            ),
            upload: upload,
            pin: nil
        )
    }

    /// `StorageInspectorFileRow` and `StorageFileDetails`.
    private static let fileScenes: [ShotScene] = [
        component("mac.component.file-row.default", height: 150) {
            VStack(spacing: 0) {
                StorageInspectorFileRow(
                    row: inspectorFile(
                        "01 Harbor Lights.flac",
                        size: 34_100_000,
                        kind: .audio(format: "FLAC")
                    )
                )
                StorageInspectorFileRow(
                    row: inspectorFile(
                        "02 The Long Way Home.flac",
                        size: 38_900_000,
                        kind: .audio(format: "FLAC"),
                        upload: BridgeFileUpload(
                            fileId: "02 The Long Way Home.flac",
                            state: .waiting,
                            pending: nil
                        )
                    )
                )
                StorageInspectorFileRow(
                    row: inspectorFile(
                        "cover.jpg",
                        size: 1_200_000,
                        kind: .image(format: "JPEG")
                    )
                )
            }
            .frame(width: 380)
        },
        component("mac.component.file-details.default", height: 220) {
            StorageFileDetails(
                file: BridgeFile(
                    id: "file-log",
                    originalFilename: "Field Recordings.log",
                    name: "Field Recordings.log",
                    folder: nil,
                    kind: .ripLog,
                    fileSize: 6_000,
                    contentType: "text/plain",
                    isImage: false,
                    audioFormat: nil
                ),
                onClose: {}
            )
            .frame(width: 380)
        },
    ]

    private static func storageChip(
        _ id: String,
        height: CGFloat = 50,
        @ViewBuilder chips: @escaping @MainActor () -> some View
    ) -> ShotScene {
        component(id, height: height) {
            HStack(spacing: 6) { chips() }
        }
    }

    /// `MacStorageChip` at rest and in the states a release's storage shows
    /// without a transfer under way.
    private static let storageChipScenes: [ShotScene] = [
        storageChip("mac.component.storage-chip.at-rest") {
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .onOneDevice
            )
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .pinned
            )
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .inStorage
            )
        },
        storageChip("mac.component.storage-chip.download-failed") {
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .downloadFailed(
                    error: "The network connection was lost."
                )
            )
        },
        storageChip("mac.component.storage-chip.move-failed") {
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .moveFailed(fileName: "01 Harbor Lights.flac")
            )
        },
        storageChip("mac.component.storage-chip.couldn-t-make-local") {
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .makeLocalFailed(
                    error: "The network connection was lost."
                )
            )
        },
        storageChip("mac.component.storage-chip.queued") {
            QueuedUploadChip()
        },
        storageChip("mac.component.storage-chip.queued-to-make-local") {
            MacStorageChip(
                release: PreviewData.storageRelease(),
                status: .makingLocal(step: .queued)
            )
        },
    ]
}
