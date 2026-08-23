import BaeKit
import SwiftUI

/// The folder the pane is about, at the top of it: what it is called on disk
/// and what audio it holds. It leads the pane because it is the one fact
/// nothing below can change — the release, the metadata and the mapping are
/// all readings of this folder.
///
/// The name is selectable (a path is something people copy) and the glyph
/// beside it is the control that shows the folder in Finder.
struct CandidateFolderLine: View {
    let folderName: String
    /// The folder on disk — what the glyph reveals.
    let folderPath: String
    /// The folder's audio shape ("FLAC", "CUE+FLAC").
    let formatLabel: String

    var body: some View {
        HStack(spacing: 8) {
            Button {
                SystemActions.revealInFinder(path: folderPath)
            } label: {
                Image(systemName: "folder")
                    .font(.system(size: 14))
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Reveal in Finder")
            Text(folderName)
                .font(.system(size: 15, design: .monospaced))
                .textSelection(.enabled)
                .lineLimit(1)
                .truncationMode(.middle)
            if !formatLabel.isEmpty {
                Text(formatLabel)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
    }
}

#if DEBUG
    #Preview("Candidate folder line") {
        CandidateFolderLine(
            folderName:
                "2010 \u{2013} Blue Sky Boys 1939\u{2013}1940 (256 kbps)",
            folderPath: "/Music/Blue Sky Boys",
            formatLabel: "FLAC"
        )
        .padding()
        .frame(width: 520)
        .windowBackground()
    }
#endif
