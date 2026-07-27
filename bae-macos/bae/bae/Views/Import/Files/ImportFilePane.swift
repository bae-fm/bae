import BaeKit
import SwiftUI

// MARK: - ImportFilePane

struct ImportFilePane: View {
    let files: BridgeCandidateFiles
    let onOpenGallery: (Int) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPreviewAudio: (String) -> Void
    /// Surface errors from file operations (e.g. readTextFile).
    let onError: (String) -> Void
    let previewState: PreviewState

    private var previewingPath: String? {
        switch previewState {
        case .idle: nil
        case .playing(let path, _): path
        case .paused(let path, _): path
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                audioSection
                imagesSection
                documentsSection
                otherSection
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            .padding(.bottom, 18)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    // MARK: - Sections (V4 — accent rules)

    /// Every audio file, each carrying the sheets that describe it. Sheets whose
    /// `FILE` directive named audio that isn't here follow, on their own — the
    /// folder still imports, and the binding is the open question.
    @ViewBuilder
    private var audioSection: some View {
        accentSection("Audio") {
            VStack(spacing: 8) {
                ForEach(files.audioFiles, id: \.file.name) { audio in
                    let sheets = files.sheets(describing: audio.file.name)
                    AudioFileRow(
                        file: audio.file,
                        sheets: sheets,
                        previewingPath: previewingPath,
                        onPreviewAudio: onPreviewAudio,
                        onOpenDocument: onOpenDocument,
                        onError: onError,
                    )
                    .accentRail(
                        isActive: !sheets.isEmpty
                            || previewingPath == audio.file.localPath
                    )
                }
                ForEach(files.unboundTrackSheets, id: \.file.name) { sheet in
                    TrackSheetRow(
                        sheet: sheet,
                        onOpenDocument: onOpenDocument,
                        onError: onError,
                    )
                    .accentRail(isActive: false)
                }
            }
        }
    }

    @ViewBuilder
    private var imagesSection: some View {
        let images = files.images
        if !images.isEmpty {
            accentSection("Images") {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 100), spacing: 8)],
                    spacing: 8
                ) {
                    ForEach(
                        Array(images.enumerated()),
                        id: \.offset
                    ) { index, file in
                        artworkCell(file, index: index)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var documentsSection: some View {
        let documents = files.documents
        if !documents.isEmpty {
            accentSection("Documents") {
                VStack(spacing: 4) {
                    ForEach(documents, id: \.file.name) { file in
                        documentRow(file.file)
                    }
                }
            }
        }
    }

    /// Files the scan doesn't recognize. The folder is the release, so these
    /// import and upload with the rest — listed here so a folder's contents are
    /// never silently partial.
    @ViewBuilder
    private var otherSection: some View {
        let other = files.other
        if !other.isEmpty {
            accentSection("Other files") {
                VStack(spacing: 4) {
                    ForEach(other, id: \.file.name) { file in
                        fileRow(icon: "doc", file: file.file)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    /// Uppercase tracked eyebrow in the accent color, hairline rule
    /// extending to the trailing edge. Replaces the chevron-collapsible
    /// section headers — the accent rules carry the hierarchy now.
    private func accentSection<Content: View>(
        _ title: LocalizedStringKey,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Text(title)
                    .font(.caption2)
                    .fontWeight(.bold)
                    .textCase(.uppercase)
                    .tracking(1.8)
                    .foregroundStyle(Theme.accent)
                Rectangle()
                    .fill(.white.opacity(0.07))
                    .frame(height: 1)
            }
            content()
        }
    }
}

// MARK: - Rows

extension ImportFilePane {
    /// 1:1 thumbnail. Tap opens the gallery at this index.
    fileprivate func artworkCell(_ file: BridgeCandidateFile, index: Int)
        -> some View
    {
        Color.clear
            .aspectRatio(1, contentMode: .fit)
            .overlay {
                ImageView(
                    source: .local(path: file.file.localPath),
                    pointSize: 200
                )
            }
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .contentShape(Rectangle())
            .gesture(
                DragGesture(
                    minimumDistance: 0,
                    coordinateSpace: .local
                )
                .onEnded { _ in
                    onOpenGallery(index)
                }
            )
    }

    fileprivate func documentRow(_ file: BridgeFileInfo) -> some View {
        Button {
            openDocument(file)
        } label: {
            fileRow(
                icon: "doc.text",
                file: file,
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    fileprivate func fileRow(
        icon: String,
        file: BridgeFileInfo,
        highlighted: Bool = false,
    ) -> some View {
        let dirPrefix = file.dirPrefix
        let fileName = file.fileName
        return VStack(spacing: 2) {
            HStack {
                Image(systemName: icon)
                    .font(.callout)
                    .foregroundStyle(
                        highlighted
                            ? AnyShapeStyle(Theme.accent)
                            : AnyShapeStyle(.secondary)
                    )
                    .frame(width: 16, alignment: .center)
                if let dirPrefix {
                    (Text(dirPrefix).foregroundColor(.secondary)
                        + Text(fileName))
                        .font(.callout)
                        .foregroundStyle(
                            highlighted
                                ? AnyShapeStyle(Theme.accent)
                                : AnyShapeStyle(.primary)
                        )
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                else {
                    Text(fileName)
                        .font(.callout)
                        .foregroundStyle(
                            highlighted
                                ? AnyShapeStyle(Theme.accent)
                                : AnyShapeStyle(.primary)
                        )
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
                Text(file.sizeText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.leading, 8)
            }
        }
    }

    fileprivate func openDocument(_ file: BridgeFileInfo) {
        do {
            let text = try readTextFile(path: file.localPath)
            onOpenDocument(file.name, text)
        }
        catch {
            onError(
                String(
                    localized:
                        "Could not read \(file.name): \(error.displayLine)"
                )
            )
        }
    }
}

// MARK: - Accent rail

extension View {
    /// 2pt accent leading rule on a row, with matching leading padding
    /// so content shifts right of the rule. When inactive the row sits
    /// flush at the same x-position via a transparent placeholder rule,
    /// keeping the layout stable across hover / preview-state changes.
    fileprivate func accentRail(isActive: Bool) -> some View {
        HStack(spacing: 0) {
            Rectangle()
                .fill(isActive ? Theme.accent : Color.clear)
                .frame(width: 2)
            self.padding(.leading, 10)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("File Pane - CUE+FLAC") {
        ImportFilePane(
            files: PreviewData.bridgeCandidateFiles,
            onOpenGallery: { _ in },
            onOpenDocument: { _, _ in },
            onPreviewAudio: { _ in },
            onError: { _ in },
            previewState: .idle,
        )
        .frame(width: 300, height: 500)
        .windowBackground()
        .environment(MediaPaths.stub)
    }

    #Preview("File Pane - Track Files") {
        ImportFilePane(
            files: PreviewData.candidateFilesTracks,
            onOpenGallery: { _ in },
            onOpenDocument: { _, _ in },
            onPreviewAudio: { _ in },
            onError: { _ in },
            previewState: .playing(
                path: "/tmp/fake/Track 3.flac",
                durationMs: 195_000
            ),
        )
        .frame(width: 300, height: 500)
        .windowBackground()
        .environment(MediaPaths.stub)
    }
#endif
