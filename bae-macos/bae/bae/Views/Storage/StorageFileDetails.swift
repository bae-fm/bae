import BaeKit
import SwiftUI

/// The details of the file selected in the storage inspector: its name, the
/// folder it sits in, what kind of file it is, its audio format and its size,
/// with the button that closes them.
struct StorageFileDetails: View {
    let file: BridgeFile
    let onClose: () -> Void

    /// The labels' column, and the inset above the details.
    private static let labelWidth: CGFloat = 84
    private static let topInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
                Text(file.name)
                    .themeText(.heading)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button(action: onClose) {
                    Image(systemName: "xmark.circle.fill")
                        .themeIcon(.medium)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Close file details"))
                .help(Text("Close file details"))
            }
            Grid(
                alignment: .leadingFirstTextBaseline,
                horizontalSpacing: ThemeSpace.group,
                verticalSpacing: ThemeSpace.compact
            ) {
                if let folder = file.folder {
                    detail("Folder") { Text(folder) }
                }
                detail("Kind") { Text(kindText) }
                if let format = file.audioFormat {
                    detail("Format") { Text(format.text) }
                }
                detail("Size") {
                    Text(file.fileSizeText).monospacedDigit()
                }
            }
            .themeText(.detail)
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.top, Self.topInset)
        .padding(.bottom, ThemeSpace.edge)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.surfaceElevated)
    }

    private func detail(
        _ label: LocalizedStringKey,
        @ViewBuilder value: () -> some View
    ) -> some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
                .frame(width: Self.labelWidth, alignment: .leading)
            value()
                .textSelection(.enabled)
        }
    }

    private var kindText: String {
        switch file.kind {
        case .audio(let format): String(localized: "\(format) audio")
        case .image(let format): String(localized: "\(format) image")
        case .cueSheet: String(localized: "Cue sheet")
        case .ripLog: String(localized: "Rip log")
        case .text: String(localized: "Text")
        case .pdf: String(localized: "PDF document")
        case .other: String(localized: "File")
        }
    }
}

#if DEBUG
    #Preview("Audio file") {
        StorageFileDetails(
            file: BridgeFile(
                id: "file-1",
                originalFilename: "Disc 1/07 Track Title.flac",
                name: "07 Track Title.flac",
                folder: "Disc 1",
                kind: .audio(format: "FLAC"),
                fileSize: 32_900_000,
                contentType: "audio/flac",
                isImage: false,
                audioFormat: BridgeAudioFormat(
                    codec: "FLAC",
                    sampleRateHz: 44_100,
                    bitsPerSample: 16,
                    bitrateKbps: nil,
                    channels: 2
                )
            ),
            onClose: {}
        )
        .frame(width: 440)
    }

    #Preview("Rip log") {
        StorageFileDetails(
            file: BridgeFile(
                id: "file-2",
                originalFilename: "Album Title.log",
                name: "Album Title.log",
                folder: nil,
                kind: .ripLog,
                fileSize: 20_000,
                contentType: "text/plain",
                isImage: false,
                audioFormat: nil
            ),
            onClose: {}
        )
        .frame(width: 440)
    }
#endif
