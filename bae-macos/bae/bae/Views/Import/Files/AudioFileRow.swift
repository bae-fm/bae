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
    /// What each nested sheet may be bound to, by the sheet's file id — see
    /// `TrackSheetRow.bindingOptions`.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// Name the audio a sheet describes: sheet file id, then the audio's, or
    /// `nil` to leave it describing nothing.
    let onBind: (String, String?) -> Void

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
                        bindingOptions: bindingOptions[sheet.file.name],
                        onBind: { onBind(sheet.file.name, $0) },
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

/// A track sheet: its name, the tracks it carves, what its `FILE` directive
/// named when that isn't in the folder, and the control that names the audio it
/// describes. Tapping the name opens the sheet in the document viewer.
///
/// The binding control is the point of this row. The scan proposes a pairing
/// from the sheet's directive; when the directive names a file that was later
/// re-encoded under another name, the user is the only one who knows the answer,
/// and this is where they give it.
struct TrackSheetRow: View {
    let sheet: BridgeCandidateFile
    let onOpenDocument: (String, String) -> Void
    let onError: (String) -> Void
    /// The audio this sheet may be bound to, each already offered or refused by
    /// core. Nil until it has been asked for; empty means there is nothing to
    /// offer, so no control appears.
    let bindingOptions: [BridgeSheetBindingOption]?
    /// Name the audio this sheet describes, or `nil` to leave it describing
    /// nothing.
    let onBind: (String?) -> Void

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
                if let requested = sheet.sheetRequestedLine {
                    Text(requested)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .padding(.top, 4)
            bindingMenu
        }
    }

    /// The audio picker. Absent while the offers are still being read, and
    /// absent when core has nothing to offer — a sheet naming one file per
    /// track, or a folder with no audio.
    @ViewBuilder
    private var bindingMenu: some View {
        if let options = bindingOptions, !options.isEmpty {
            Menu {
                ForEach(options, id: \.fileId) { option in
                    bindButton(option)
                }
                Divider()
                Button {
                    onBind(nil)
                } label: {
                    checkable(
                        coreString("ui.import.sheet.describes_nothing"),
                        selected: sheet.describes == nil
                    )
                }
            } label: {
                Label(
                    sheet.describes
                        ?? coreString("ui.import.sheet.choose_audio"),
                    systemImage: "link"
                )
                .font(.caption2)
                .lineLimit(1)
                .truncationMode(.middle)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
        }
    }

    /// One offered file, or a refused one shown disabled with core's reason —
    /// visible rather than hidden, so a folder whose only audio the sheet can't
    /// use reads as "here is why" instead of an empty menu.
    @ViewBuilder
    private func bindButton(_ option: BridgeSheetBindingOption) -> some View {
        if let refusal = option.refusalLine {
            Button {
            } label: {
                Text("\(option.fileId) — \(refusal)")
            }
            .disabled(true)
        }
        else {
            Button {
                onBind(option.fileId)
            } label: {
                checkable(
                    option.fileId,
                    selected: sheet.describes == option.fileId
                )
            }
        }
    }

    @ViewBuilder
    private func checkable(_ label: String, selected: Bool) -> some View {
        if selected {
            Label(label, systemImage: "checkmark")
        }
        else {
            Text(label)
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
            bindingOptions: PreviewData.sheetBindingOptions,
            onBind: { _, _ in },
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
                bindingOptions: PreviewData.sheetBindingOptions[
                    PreviewData.unboundTrackSheet.file.name
                ],
                onBind: { _ in },
            )
            TrackSheetRow(
                sheet: PreviewData.refusedTrackSheet,
                onOpenDocument: { _, _ in },
                onError: { _ in },
                bindingOptions: PreviewData.sheetBindingOptions[
                    PreviewData.unboundTrackSheet.file.name
                ],
                onBind: { _ in },
            )
        }
        .padding()
        .frame(width: 300)
        .windowBackground()
    }
#endif
