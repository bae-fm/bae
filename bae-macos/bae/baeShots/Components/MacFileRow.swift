import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFileRow`, a file of the inspector at rest, waiting, moving,
    /// selected, the file a move stopped on because its original changed,
    /// and the file an upload waits for because its original is missing.
    static let macFileRowScenes: [ShotScene] = [
        fileRow(
            "mac.component.file-row.at-rest",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC")
            )
        ),
        fileRow(
            "mac.component.file-row.queued",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                upload: BridgeFileUpload(
                    fileId: "01 Harbor Lights.flac",
                    state: .waiting,
                    pending: nil
                )
            )
        ),
        fileRow(
            "mac.component.file-row.queued-to-pin",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                download: .waiting(kind: .pin)
            )
        ),
        fileRow(
            "mac.component.file-row.queued-to-move",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                download: .waiting(kind: .moveToThisDevice)
            )
        ),
        fileRow(
            "mac.component.file-row.sending",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                upload: BridgeFileUpload(
                    fileId: "01 Harbor Lights.flac",
                    state: .uploading,
                    pending: nil
                )
            )
        ),
        fileRow(
            "mac.component.file-row.pinning",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                download: .downloading(
                    kind: .pin,
                    fileId: "01 Harbor Lights.flac"
                )
            )
        ),
        fileRow(
            "mac.component.file-row.moving",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                download: .downloading(
                    kind: .moveToThisDevice,
                    fileId: "moved-file"
                )
            )
        ),
        fileRow(
            "mac.component.file-row.error",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                problem: .originalChanged
            )
        ),
        fileRow(
            "mac.component.file-row.waiting",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                problem: .originalMissing
            )
        ),
        fileRow(
            "mac.component.file-row.selected",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC")
            ),
            selected: true
        ),
        fileRow(
            "mac.component.file-row.long-name",
            inspectorFile(
                "01 Harbor Lights (Recorded Live at the Harbor Hall, Second Night).flac",
                size: 98_400_000,
                kind: .audio(format: "FLAC")
            )
        ),
    ]

    private static func inspectorFile(
        _ name: String,
        size: UInt64,
        kind: BridgeFileKind,
        upload: BridgeFileUpload? = nil,
        download: BridgeFileDownload? = nil,
        problem: BridgeFileProblem? = nil
    ) -> BridgeStorageInspectorFile {
        BridgeStorageInspectorFile(
            file: releaseFile(name, size: size, kind: kind),
            upload: upload,
            download: download,
            problem: problem
        )
    }

    private static func fileRow(
        _ id: String,
        _ row: BridgeStorageInspectorFile,
        selected: Bool = false
    ) -> ShotScene {
        component(id, height: 50) {
            Group {
                if selected {
                    AccentSelectionList(
                        rows: [row],
                        id: \.file.id,
                        selection: Binding<String?>.constant(row.file.id)
                    ) { row in
                        MacFileRow(row: row)
                    }
                    .frame(height: 36)
                }
                else {
                    MacFileRow(row: row)
                }
            }
            .frame(width: 380, alignment: .leading)
            .environment(
                OutboxStore(
                    uploadsPaused: false,
                    fileShares: .fileShares(
                        holding: ["01 Harbor Lights.flac": 0.7]
                    )
                )
            )
            .environment(
                DownloadStore(
                    fileShares: .fileShares(
                        holding: ["01 Harbor Lights.flac": 0.4]
                    ),
                    moveFileShares: .fileShares(holding: ["moved-file": 0.4])
                )
            )
        }
    }
}
