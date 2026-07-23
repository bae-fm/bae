import BaeKit
import SwiftUI

/// A work's detail: its child works, the releases that carry it, and its
/// recordings. Child works and releases open on tap; recordings are text-only.
struct WorkDetailView: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(detail.work.title)
                .font(.system(size: 18, weight: .bold))
                .tracking(-0.2)
                .lineLimit(2)
            if !detail.childWorks.isEmpty {
                SectionHeader(title: String(localized: "Works"))
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(detail.childWorks, id: \.workId) { work in
                        Button(action: { openWork(work.workId) }) {
                            DetailMediaRow(
                                image: work.representativeCover,
                                title: work.title,
                                subtitle: work.composerNames
                            )
                        }
                        .buttonStyle(DetailRowButtonStyle())
                    }
                }
            }
            if !detail.releases.isEmpty {
                SectionHeader(title: String(localized: "Releases"))
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(detail.releases, id: \.releaseId) { release in
                        Button(action: {
                            openAlbum(release.albumId, release.releaseId)
                        }) {
                            DetailMediaRow(
                                image: release.cover,
                                title: release.albumTitle,
                                subtitle: workReleaseMetadata(release)
                            )
                        }
                        .buttonStyle(DetailRowButtonStyle())
                    }
                }
            }
            if !detail.tracks.isEmpty {
                SectionHeader(title: String(localized: "Recordings"))
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(detail.tracks, id: \.trackId) { track in
                        CreditRow(
                            title: track.trackTitle,
                            subtitle: track.albumTitle
                        )
                    }
                }
            }
        }
    }

    private func workReleaseMetadata(
        _ release: BridgeWorkReleaseSummary
    ) -> String {
        precondition(
            !release.displayName.isEmpty,
            "work release display name is empty for \(release.releaseId)"
        )
        if let format = release.format, !format.isEmpty {
            return "\(release.displayName) \u{00B7} \(format)"
        }
        return release.displayName
    }
}
