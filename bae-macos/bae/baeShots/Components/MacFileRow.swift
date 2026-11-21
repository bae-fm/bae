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
                "cover.jpg",
                size: 1_200_000,
                kind: .image(format: "JPEG")
            )
        ),
        fileRow(
            "mac.component.file-row.queued",
            inspectorFile(
                "02 The Long Way Home.flac",
                size: 38_900_000,
                kind: .audio(format: "FLAC"),
                upload: BridgeFileUpload(
                    fileId: "02 The Long Way Home.flac",
                    state: .waiting,
                    pending: nil
                )
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
                "03 Second Floor.flac",
                size: 28_200_000,
                kind: .audio(format: "FLAC"),
                pin: .downloading(fileId: "03 Second Floor.flac")
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
                "01 Evening Lines.flac",
                size: 31_000_000,
                kind: .audio(format: "FLAC"),
                problem: .originalMissing
            )
        ),
        fileRow(
            "mac.component.file-row.selected",
            inspectorFile(
                "Low Tide Hours.cue",
                size: 2_000,
                kind: .cueSheet
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
        pin: BridgeFilePin? = nil,
        problem: BridgeFileProblem? = nil
    ) -> BridgeStorageInspectorFile {
        BridgeStorageInspectorFile(
            file: releaseFile(name, size: size, kind: kind),
            upload: upload,
            pin: pin,
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
                        holding: ["03 Second Floor.flac": 0.4]
                    )
                )
            )
        }
    }
}
