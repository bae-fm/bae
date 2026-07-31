import BaeKit
import SwiftUI

/// Zone 1 of the mapping pane: the cover, what the release is, and the claim
/// this import records.
///
/// Search is this header's editor rather than a pane mounted beside it — the
/// change control opens it, and picking a release replaces the slot table's
/// source-side column live. Before anything is picked the same control reads
/// "Find this release".
struct ImportReleaseHeader: View {
    let title: String
    let artist: String
    /// "CD · 1996 · 9 tracks", from the live editor, so it tracks what is being
    /// edited rather than what was fetched.
    let metaLine: String
    /// What this import claims to hold and where its metadata came from, as
    /// core derived it. `nil` before a pick, and for an Unknown import, which
    /// claims nothing.
    let claim: BridgeClaimLine?
    /// Whether a release has been picked.
    let hasPick: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    let onEditCover: () -> Void
    let onFindRelease: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 16) {
                cover
                summary
                changeControl
            }
            if let claim {
                ImportClaimLine(claim: claim)
            }
        }
        .padding(14)
        .formGroupCard()
    }

    /// The header's editor, opened. Prominent while nothing is picked — it is
    /// the one thing left to do — and quiet once a release is in.
    @ViewBuilder
    private var changeControl: some View {
        if hasPick {
            Button(coreString("ui.import.header.change_release")) {
                onFindRelease()
            }
            .buttonStyle(.bordered)
        }
        else {
            Button(coreString("ui.import.header.find_release")) {
                onFindRelease()
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.system(size: 17, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.tail)
            Text(artist)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Text(metaLine)
                .font(.system(size: 11.5))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var cover: some View {
        Group {
            if let coverContent {
                ImageView(content: coverContent, pointSize: 80)
            }
            else {
                Theme.placeholder
            }
        }
        .frame(width: 80, height: 80)
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay(alignment: .topTrailing) {
            if hasCoverOptions {
                Image(systemName: "pencil")
                    .font(.caption2)
                    .foregroundStyle(.white)
                    .padding(3)
                    .background(.black.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: 3))
                    .padding(2)
            }
        }
        .onTapGesture {
            if hasCoverOptions {
                onEditCover()
            }
        }
    }
}
