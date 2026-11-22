import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacItem`, one shot for each row of its preview.
    static let macItemScenes: [ShotScene] = [
        // `MacItem` among a list's rows, with a failed transfer's notice between
        // two files.
        component("mac.component.item.among-rows-with-a-notice", height: 210) {
            let op = BridgeDownloadOp(
                releaseId: "rel-glass-harbor",
                kind: .pin,
                fileCount: 9,
                totalSize: 356_000_000,
                release: BridgeQueuedRelease(title: "Glass Harbor"),
                createdAt: 0,
                state: .failed(error: "The network connection was lost.")
            )
            VStack(spacing: 0) {
                itemFile("01 Harbor Lights.flac", size: 34_100_000)
                MacFailedTransferRow(
                    entry: .download(op, onRetry: {}),
                    onCancel: {}
                )
                itemFile("02 The Long Way Home.flac", size: 38_900_000)
            }
            .frame(width: 380, alignment: .leading)
        },
        // `MacItem`, a row's lead, text and trailing end.
        component("mac.component.item.icon-text-trailing", height: 60) {
            MacItem {
                StatusGlyph(.danger).themeIcon(.medium)
                    .accessibilityHidden(true)
            } content: {
                Text("Download failed")
            } trail: {
                Text("2m ago")
            }
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.item.empty-lead-keeps-the-edge", height: 100) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                MacItem {
                    StatusGlyph(.warning).themeIcon(.medium)
                        .accessibilityHidden(true)
                } content: {
                    Text("Dropbox signed this device out.")
                }
                MacItem {
                } content: {
                    Text("01 Harbor Lights.flac")
                } trail: {
                    Text("34.1 MB").themeText(.fine)
                }
            }
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.item.two-lines", height: 70) {
            MacItem {
                StatusGlyph(.info).themeIcon(.medium)
                    .accessibilityHidden(true)
            } content: {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text("3 uploading")
                    Text("96 MB of 221 MB")
                        .themeText(.detail)
                        .foregroundStyle(Theme.macTextSecondary)
                }
            }
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.item.center-lead", height: 60) {
            itemCentered()
        },
        component("mac.component.item.center-trail", height: 60) {
            itemCentered()
        },
        component("mac.component.item.wide", height: 60) {
            MacItem(wide: true) {
                StatusGlyph(.danger).themeIcon(.medium)
                    .accessibilityHidden(true)
            } content: {
                Text("Download failed")
            } trail: {
                Text("2m ago")
            }
            .frame(width: 380, alignment: .leading)
        },
    ]

    /// A download's row with its lead or its trailing end set in the middle.
    /// `MacItem` centres both together, so each state is the item centred.
    private static func itemCentered() -> some View {
        MacItem(centered: true) {
            StatusGlyph(.danger).themeIcon(.medium)
                .accessibilityHidden(true)
        } content: {
            Text("Download failed")
        } trail: {
            Text("2m ago")
        }
        .frame(width: 380, alignment: .leading)
    }

    /// A stored audio file's row.
    private static func itemFile(_ name: String, size: UInt64) -> MacFileRow {
        MacFileRow(
            row: BridgeStorageInspectorFile(
                file: BridgeFile(
                    id: name,
                    originalFilename: name,
                    name: name,
                    folder: nil,
                    kind: .audio(format: "FLAC"),
                    fileSize: size,
                    contentType: "application/octet-stream",
                    isImage: false,
                    audioFormat: nil
                ),
                upload: nil,
                pin: nil,
                problem: nil
            )
        )
    }
}
