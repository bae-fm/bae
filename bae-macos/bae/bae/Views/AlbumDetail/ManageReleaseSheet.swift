import BaeKit
import SwiftUI

/// The Storage sheet for a single release: its storage status band plus a
/// sortable table of the release's files. Presented from the album detail's
/// menu.
struct ManageReleaseSheet: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    let onDone: () -> Void

    /// Column the file table sorts by. Defaults to filename; the user clicks a
    /// column header to re-sort. The Format column is display-only (its value is
    /// optional, so there's no natural ordering key) — sort by Kind to group by
    /// content type.
    @State
    private var sortOrder: [KeyPathComparator<BridgeFile>] = [
        KeyPathComparator(\BridgeFile.originalFilename)
    ]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Storage")
                    .font(.headline)
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()
            Divider()
            StorageStatusBand(
                release: release,
                onAction: onAction,
                onExport: onExport,
                onSaveAs: onSaveAs
            )
            Divider()
            filesSection
        }
    }

    private var filesSection: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Files")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)
            .padding(.bottom, 4)

            Table(release.files.sorted(using: sortOrder), sortOrder: $sortOrder)
            {
                TableColumn("Name", value: \.originalFilename) { file in
                    Text(file.originalFilename).lineLimit(1)
                }
                TableColumn("Format") { file in
                    // Audio files carry a label; non-audio files (images, cue)
                    // have none, so their Format cell is simply empty.
                    if let format = file.audioFormat {
                        Text(format.text)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                TableColumn("Size", value: \.fileSize) { file in
                    Text(file.fileSizeText)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .width(min: 70, ideal: 80)
                TableColumn("Kind", value: \.contentType) { file in
                    Text(file.contentType)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
                .width(min: 90, ideal: 130)
            }
        }
    }
}
