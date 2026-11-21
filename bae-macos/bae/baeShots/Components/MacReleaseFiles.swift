import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleaseFiles`, one shot for each row of its preview.
    static let macReleaseFilesScenes: [ShotScene] = [
        releaseFilesScene(
            "mac.component.release-files.files",
            height: 120,
            rows: [
                fileRow("rip.log", size: 4_000, role: .document),
                fileRow("notes.txt", size: 2_000, role: .document),
            ]
        ),
        releaseFilesScene(
            "mac.component.release-files.skipped-folder",
            height: 200,
            rows: [
                fileRow(
                    "01 Northbound.flac",
                    size: 33_000_000,
                    role: .audio,
                    plays: true
                ),
                fileRow(
                    "02 Margins.flac",
                    size: 31_800_000,
                    role: .damagedAudio(damage: .unreadable)
                ),
                fileRow("rip.log", size: 4_000, role: .document),
                fileRow(".DS_Store", size: 6_000, role: .other),
            ]
        ),
        releaseFilesScene(
            "mac.component.release-files.one-file",
            height: 80,
            rows: [fileRow("rip.log", size: 4_000, role: .document)]
        ),
        releaseFilesScene(
            "mac.component.release-files.image-bae-cannot-show",
            height: 120,
            rows: [
                fileRow("rip.log", size: 4_000, role: .document),
                fileRow("scan.bmp", size: 2_400_000, role: .other),
            ]
        ),
    ]

    private static func fileRow(
        _ name: String,
        size: UInt64,
        role: BridgeMappingRole,
        plays: Bool = false
    ) -> BridgeMappingFileRow {
        .file(
            file: BridgeMappingFile(
                fileId: name,
                name: name,
                size: size,
                previewTarget: plays ? previewTarget(name) : nil,
                role: role
            )
        )
    }

    /// `MacReleaseFiles` over `rows`, in the preview's 520pt column.
    private static func releaseFilesScene(
        _ id: String,
        height: CGFloat,
        rows: [BridgeMappingFileRow]
    ) -> ShotScene {
        component(
            id,
            width: 520,
            height: height
        ) {
            MacReleaseFiles(
                rows: rows,
                onOpenDocument: { _, _ in },
                playing: (nil, inertMappingActions().audition)
            )
            .frame(width: 520, alignment: .leading)
        }
    }
}
