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

/// How the folder this candidate came out of was read, and the control that
/// reads it the other way. The scan reads such a folder for itself so the queue
/// has candidates to work on; this is where that reading is visible and where
/// it is overruled.
struct FolderReadingControl: View {
    let boundary: BridgeResolvedFolderReleaseBoundary
    let onDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void

    var body: some View {
        HStack(spacing: 8) {
            Label(boundary.name, systemImage: "folder")
                .font(.system(size: 11.5))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            switch boundary.decision {
            case .combineAsOneRelease:
                Button("Keep as Separate Releases") {
                    onDecision(boundary.key, .keepAsSeparateReleases)
                }
            case .keepAsSeparateReleases:
                Button("Combine as One Release") {
                    onDecision(boundary.key, .combineAsOneRelease)
                }
            }
            Spacer(minLength: 0)
        }
        .controlSize(.small)
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

    #Preview("Folder reading") {
        VStack(alignment: .leading, spacing: 8) {
            FolderReadingControl(
                boundary: BridgeResolvedFolderReleaseBoundary(
                    key: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: "/Music",
                        relativeFolderPath: "Blue Sky Boys"
                    ),
                    decision: .combineAsOneRelease,
                    name: "Blue Sky Boys",
                    displayPath: "Blue Sky Boys"
                ),
                onDecision: { _, _ in }
            )
            FolderReadingControl(
                boundary: BridgeResolvedFolderReleaseBoundary(
                    key: BridgeFolderReleaseDecisionKey(
                        watchedFolderPath: "/Music",
                        relativeFolderPath: "Rarities"
                    ),
                    decision: .keepAsSeparateReleases,
                    name: "Rarities",
                    displayPath: "Rarities"
                ),
                onDecision: { _, _ in }
            )
        }
        .padding()
        .frame(width: 520)
        .windowBackground()
    }
#endif
