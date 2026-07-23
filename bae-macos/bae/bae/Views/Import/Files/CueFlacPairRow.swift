import BaeKit
import SwiftUI

/// One CUE+FLAC pair in the import file pane: the FLAC (tappable to audition,
/// with its total size), the CUE sheet below it (tappable to open in the
/// document viewer), and a track-count capsule. The FLAC highlights while it's
/// the audition target.
struct CueFlacPairRow: View {
    let pair: BridgeCueFlacPair
    /// The path currently auditioning, if any — highlights the FLAC when it
    /// matches.
    let previewingPath: String?
    let onPreviewAudio: (String) -> Void
    let onOpenDocument: (String, String) -> Void
    /// Surface errors from reading the CUE sheet.
    let onError: (String) -> Void

    private var isPreviewingFlac: Bool {
        previewingPath == pair.flacLocalPath
    }

    var body: some View {
        VStack(spacing: 4) {
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
                            Text(pair.totalSizeText)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .padding(.leading, 8)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    cueFileRow
                    Text("\(Int(pair.trackCount)) tracks")
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

    private var cueFileRow: some View {
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
                        String(
                            localized:
                                "Could not read \(pair.cueName): \(error.displayLine)"
                        )
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
            Text(pair.cueSizeText)
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.leading, 8)
        }
    }
}

#if DEBUG
    #Preview("CUE+FLAC row") {
        CueFlacPairRow(
            pair: BridgeCueFlacPair(
                cueName: "Album Title.cue",
                cueSize: 1200,
                cueLocalPath: "/tmp/fake/Album Title.cue",
                flacName: "Album Title.flac",
                flacLocalPath: "/tmp/fake/Album Title.flac",
                totalSize: 340_000_000,
                trackCount: 9,
            ),
            previewingPath: "/tmp/fake/Album Title.flac",
            onPreviewAudio: { _ in },
            onOpenDocument: { _, _ in },
            onError: { _ in },
        )
        .padding()
        .frame(width: 300)
        .windowBackground()
    }
#endif
