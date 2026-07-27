import BaeKit
import SwiftUI

/// One audio file in the import file pane, with the track sheets that describe
/// it nested underneath. Tapping the audio auditions it; tapping a sheet opens
/// it in the document viewer. The audio highlights while it's the audition
/// target.
///
/// This is where a CUE+FLAC rip reads as what it is: one audio file and one
/// sheet that describes it, related but separate rows.
struct AudioFileRow: View {
    let file: BridgeFileInfo
    /// The sheets bound to this file. Empty for a plain track.
    let sheets: [BridgeCandidateFile]
    /// The path currently auditioning, if any — highlights the audio when it
    /// matches.
    let previewingPath: String?
    let onPreviewAudio: (String) -> Void
    let onOpenDocument: (String, String) -> Void
    /// Surface errors from reading a sheet.
    let onError: (String) -> Void

    private var isPreviewing: Bool { previewingPath == file.localPath }

    var body: some View {
        HStack(alignment: .top) {
            Image(systemName: icon)
                .font(.callout)
                .foregroundStyle(
                    isPreviewing || !sheets.isEmpty
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(.secondary)
                )
                .frame(width: 20, alignment: .center)
            VStack(alignment: .leading, spacing: 2) {
                Button {
                    onPreviewAudio(file.localPath)
                } label: {
                    HStack {
                        Text(file.fileName)
                            .font(.callout)
                            .foregroundStyle(
                                isPreviewing
                                    ? AnyShapeStyle(Theme.accent)
                                    : AnyShapeStyle(.primary)
                            )
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer()
                        Text(file.sizeText)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(.leading, 8)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                ForEach(sheets, id: \.file.name) { sheet in
                    TrackSheetRow(
                        sheet: sheet,
                        onOpenDocument: onOpenDocument,
                        onError: onError,
                    )
                }
            }
        }
    }

    /// A disc glyph once a sheet describes this file — it's a disc image, not a
    /// track — and a waveform otherwise.
    private var icon: String {
        if isPreviewing {
            return "speaker.wave.2.fill"
        }
        return sheets.isEmpty ? "waveform" : "opticaldisc"
    }
}

/// A track sheet: its name, the tracks it carves, and — when its `FILE`
/// directive named audio that isn't in the folder — the line saying so. Tapping
/// opens the sheet in the document viewer.
struct TrackSheetRow: View {
    let sheet: BridgeCandidateFile
    let onOpenDocument: (String, String) -> Void
    let onError: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Button {
                    open()
                } label: {
                    Text(sheet.file.fileName)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .buttonStyle(.plain)
                Spacer()
                Text(sheet.file.sizeText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.leading, 8)
            }
            HStack(spacing: 6) {
                Text("\(Int(sheet.sheetTrackCount)) tracks")
                    .font(.caption2)
                    .fontWeight(.semibold)
                    .foregroundStyle(Theme.accent)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 1)
                    .background(Theme.accentSoft, in: Capsule())
                if let unbound = sheet.unboundSheetLine {
                    Text(unbound)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.top, 4)
        }
    }

    private func open() {
        do {
            let text = try readTextFile(path: sheet.file.localPath)
            onOpenDocument(sheet.file.name, text)
        }
        catch {
            onError(
                String(
                    localized:
                        "Could not read \(sheet.file.name): \(error.displayLine)"
                )
            )
        }
    }
}

#if DEBUG
    #Preview("Audio + bound sheet") {
        AudioFileRow(
            file: BridgeFileInfo(
                name: "Album Title.flac",
                size: 340_000_000,
                dirPrefix: nil,
                fileName: "Album Title.flac",
                localPath: "/tmp/fake/Album Title.flac"
            ),
            sheets: [PreviewData.boundTrackSheet],
            previewingPath: "/tmp/fake/Album Title.flac",
            onPreviewAudio: { _ in },
            onOpenDocument: { _, _ in },
            onError: { _ in },
        )
        .padding()
        .frame(width: 300)
        .windowBackground()
    }

    #Preview("Sheet describing nothing yet") {
        VStack(alignment: .leading, spacing: 16) {
            TrackSheetRow(
                sheet: PreviewData.unboundTrackSheet,
                onOpenDocument: { _, _ in },
                onError: { _ in },
            )
            TrackSheetRow(
                sheet: PreviewData.refusedTrackSheet,
                onOpenDocument: { _, _ in },
                onError: { _ in },
            )
        }
        .padding()
        .frame(width: 300)
        .windowBackground()
    }
#endif
