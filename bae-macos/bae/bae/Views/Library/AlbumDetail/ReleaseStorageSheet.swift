import BaeKit
import SwiftUI

/// A release's Storage sheet: its storage status band over a sortable table of
/// its files.
struct ReleaseStorageSheet: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    let onDone: () -> Void

    /// The file table's sort; the Audio column has none, as its value is
    /// optional.
    @State
    private var sortOrder: [KeyPathComparator<BridgeFile>] = [
        // Finder's order: case-insensitive and numeric-aware, the same order
        // core hands the files over in.
        KeyPathComparator(
            \BridgeFile.originalFilename,
            comparator: .localizedStandard
        )
    ]

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Storage")
                    .themeText(.heading)
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
                Eyebrow("Files")
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, 8)
            .padding(.bottom, 4)

            Table(release.files.sorted(using: sortOrder), sortOrder: $sortOrder)
            {
                TableColumn(
                    "Name",
                    value: \.originalFilename,
                    comparator: .localizedStandard
                ) { file in
                    Text(file.originalFilename).lineLimit(1)
                }
                TableColumn(coreString("core.audio.label")) { file in
                    // Non-audio files (images, cue) leave the cell empty.
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

#if DEBUG
    #Preview("Release Storage Sheet") {
        ReleaseStorageSheet(
            release: PreviewData.storageRelease(
                storageState: .remote,
                pinned: false,
                storageActions: [.pin, .makeLocal]
            ),
            onAction: { _ in },
            onExport: {},
            onSaveAs: {},
            onDone: {},
        )
        .frame(width: 640, height: 600)
        .background(Theme.background)
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .preferredColorScheme(.dark)
    }
#endif
