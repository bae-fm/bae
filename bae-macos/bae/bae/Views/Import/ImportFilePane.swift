import SwiftUI

// MARK: - ImportFilePane

struct ImportFilePane: View {
    let files: CandidateFiles
    let onOpenGallery: (Int) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPreviewAudio: (String) -> Void
    /// Surface errors from file operations (e.g. readTextFile).
    let onError: (String) -> Void
    let previewState: PreviewState

    private var previewingPath: String? {
        switch previewState {
        case .idle: nil
        case .playing(let path, _, _): path
        case .paused(let path, _, _): path
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                audioSection
                imagesSection
                documentsSection
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            .padding(.bottom, 18)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    // MARK: - Sections (V4 — accent rules)

    @ViewBuilder
    private var audioSection: some View {
        accentSection("Audio") {
            switch files.audio {
            case .cueFlacPairs(let pairs):
                VStack(spacing: 8) {
                    ForEach(Array(pairs.enumerated()), id: \.offset) {
                        _,
                        pair in
                        cueFlacPairRow(pair).accentRail(isActive: true)
                    }
                }
            case .trackFiles(let files):
                VStack(spacing: 4) {
                    ForEach(Array(files.enumerated()), id: \.offset) {
                        _,
                        file in
                        trackRow(file)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var imagesSection: some View {
        if !files.artwork.isEmpty {
            accentSection("Images") {
                LazyVGrid(
                    columns: [GridItem(.adaptive(minimum: 100), spacing: 8)],
                    spacing: 8
                ) {
                    ForEach(
                        Array(files.artwork.enumerated()),
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
        if !files.documents.isEmpty {
            accentSection("Documents") {
                VStack(spacing: 4) {
                    ForEach(
                        Array(files.documents.enumerated()),
                        id: \.offset
                    ) { _, file in
                        documentRow(file)
                    }
                }
            }
        }
    }

    /// Uppercase tracked eyebrow in the accent color, hairline rule
    /// extending to the trailing edge. Replaces the chevron-collapsible
    /// section headers — the accent rules carry the hierarchy now.
    private func accentSection<Content: View>(
        _ title: String,
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

    // MARK: - Artwork cell

    /// 1:1 thumbnail. Tap opens the gallery at this index.
    private func artworkCell(_ file: ArtworkFile, index: Int)
        -> some View
    {
        Color.clear
            .aspectRatio(1, contentMode: .fit)
            .overlay {
                ImageView(source: .local(path: file.localPath), pointSize: 200)
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

    private func trackRow(_ file: FileInfo) -> some View {
        let isPreviewing = previewingPath == file.localPath
        return Button {
            handleTap(file: file, kind: .audio)
        } label: {
            fileRow(
                icon: isPreviewing ? "speaker.wave.2.fill" : "waveform",
                file: file,
                highlighted: isPreviewing,
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accentRail(isActive: isPreviewing)
    }

    private func documentRow(_ file: FileInfo) -> some View {
        Button {
            handleTap(file: file, kind: .document)
        } label: {
            fileRow(
                icon: "doc.text",
                file: file,
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func cueFlacPairRow(_ pair: CueFlacPair) -> some View {
        let isPreviewingFlac = previewingPath == pair.flacLocalPath

        return VStack(spacing: 4) {
            HStack(alignment: .top) {
                Image(
                    systemName: isPreviewingFlac
                        ? "speaker.wave.2.fill" : "opticaldisc"
                )
                .font(.callout)
                .foregroundStyle(Theme.accent)
                .frame(width: 20, alignment: .center)
                VStack(alignment: .leading, spacing: 2) {
                    Button {
                        onPreviewAudio(pair.flacLocalPath)
                    } label: {
                        HStack {
                            Text(pair.flacName)
                                .font(.callout)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Spacer()
                            Text(pair.totalSizeLabel)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .padding(.leading, 8)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    HStack {
                        Button {
                            do {
                                let text = try readTextFile(
                                    path: pair.cueLocalPath
                                )
                                onOpenDocument(pair.cueName, text)
                            }
                            catch {
                                onError(
                                    "Could not read \(pair.cueName): \(error.localizedDescription)"
                                )
                            }
                        } label: {
                            Text(pair.cueName)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        .buttonStyle(.plain)
                        Spacer()
                        Text(pair.cueSizeLabel)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(.leading, 8)
                    }
                    if let trackCount = pair.trackCount {
                        Text("\(trackCount) tracks")
                            .font(.caption2)
                            .fontWeight(.semibold)
                            .foregroundStyle(Theme.accent)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 1)
                            .background(
                                Theme.accentSoft,
                                in: Capsule()
                            )
                            .padding(.top, 4)
                    }
                }
            }
        }
    }

    private func fileRow(
        icon: String,
        file: FileInfo,
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
                Text(file.sizeLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.leading, 8)
            }
        }
    }

    private enum FileKind { case audio, document }

    private func handleTap(file: FileInfo, kind: FileKind) {
        switch kind {
        case .audio: onPreviewAudio(file.localPath)
        case .document:
            do {
                let text = try readTextFile(path: file.localPath)
                onOpenDocument(file.name, text)
            }
            catch {
                onError(
                    "Could not read \(file.name): \(error.localizedDescription)"
                )
            }
        }
    }
}

// MARK: - DocumentViewerView

struct DocumentViewerView: View {
    let name: String
    let text: String
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(name)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Done") { onClose() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            Divider()
            ScrollView {
                Text(text)
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding()
            }
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

// MARK: - Previews

#Preview("File Pane - CUE+FLAC") {
    ImportFilePane(
        files: PreviewData.candidateFiles,
        onOpenGallery: { _ in },
        onOpenDocument: { _, _ in },
        onPreviewAudio: { _ in },
        onError: { _ in },
        previewState: .idle,
    )
    .frame(width: 300, height: 500)
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
            durationMs: 195_000,
            durationLabel: "3:15"
        ),
    )
    .frame(width: 300, height: 500)
    .environment(MediaPaths.stub)
}

#Preview("Document Viewer") {
    DocumentViewerView(
        name: "info.txt",
        text:
            "This is sample document content.\nLine 2 of the document.\nLine 3 with more text.",
        onClose: {},
    )
    .frame(width: 600, height: 500)
    .background(Theme.surface)
}
