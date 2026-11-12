import BaeKit
import SwiftUI

/// A work's detail: its child works, the releases that carry it, and its
/// recordings. Child works and releases open on tap; recordings are text-only.
struct WorkDetailView: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(detail.work.title)
                .themeText(.title)
                .lineLimit(2)
            if !detail.childWorks.isEmpty {
                SectionHeader(title: String(localized: "Works"))
                VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                    ForEach(detail.childWorks, id: \.workId) { work in
                        MacLibraryRowButton(
                            picture: work.representativeCover,
                            title: work.title,
                            subtitle: work.composerNames,
                            bleed: true,
                            action: { openWork(work.workId) }
                        )
                    }
                }
            }
            if !detail.releases.isEmpty {
                SectionHeader(title: String(localized: "Releases"))
                VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                    ForEach(detail.releases, id: \.releaseId) { release in
                        MacLibraryRowButton(
                            picture: release.cover,
                            title: release.albumTitle,
                            subtitle: workReleaseMetadata(release),
                            bleed: true,
                            action: {
                                openAlbum(release.albumId, release.releaseId)
                            }
                        )
                    }
                }
            }
            if !detail.tracks.isEmpty {
                SectionHeader(title: String(localized: "Recordings"))
                VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                    ForEach(detail.tracks, id: \.trackId) { track in
                        MacLibraryRow(
                            title: track.trackTitle,
                            subtitle: track.albumTitle,
                            bleed: true,
                            credit: true
                        )
                    }
                }
            }
        }
    }

    private func workReleaseMetadata(
        _ release: BridgeWorkReleaseSummary
    ) -> String {
        release.metadataText
    }
}
