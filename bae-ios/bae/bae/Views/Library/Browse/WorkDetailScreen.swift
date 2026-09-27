import BaeKit
import SwiftUI

/// A work's child works, releases, and recordings, loaded on demand by work id.
struct WorkDetailScreen: View {
    let workId: String
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    @Environment(LibraryProjectionStore.self)
    private var libraryProjections

    private var detail: BridgeWorkDetail? { libraryProjections.work.value }
    private var error: String? {
        libraryProjections.work.error?.line
            ?? (libraryProjections.work.delivered && detail == nil
                ? String(localized: "Work detail not found") : nil)
    }

    var body: some View {
        Group {
            if let detail {
                WorkDetailContent(
                    detail: detail,
                    openWork: openWork,
                    openAlbum: openAlbum
                )
                .overlay(alignment: .top) {
                    if let error {
                        Text(error).foregroundStyle(Theme.danger).padding(ThemeSpace.group)
                    }
                }
            }
            else if let error {
                Text(error).foregroundStyle(Theme.danger).padding(ThemeSpace.page)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { libraryProjections.activateWork(workId) }
        .onDisappear { libraryProjections.deactivateWork(workId) }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.work.title
        }
        if error != nil {
            return String(localized: "Works")
        }
        return ""
    }

}

private struct WorkDetailContent: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    var body: some View {
        List {
            Section {
                WorkSummaryRow(summary: detail.work)
            }
            if !detail.childWorks.isEmpty {
                Section("Works") {
                    ForEach(detail.childWorks, id: \.workId) { work in
                        WorkSummaryButton(summary: work, openWork: openWork)
                    }
                }
            }
            if !detail.releases.isEmpty {
                Section("Releases") {
                    ForEach(detail.releases, id: \.releaseId) { release in
                        Button {
                            openAlbum(release)
                        } label: {
                            HStack(spacing: ThemeSpace.group) {
                                ImageView(imageRef: release.cover, pointSize: ThemeSize.rowArtwork)
                                    .frame(
                                        width: ThemeSize.rowArtwork,
                                        height: ThemeSize.rowArtwork
                                    )
                                    .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                                TwoLineRow(
                                    title: release.albumTitle,
                                    subtitle: workReleaseMetadata(release)
                                )
                            }
                        }
                    }
                }
            }
            if !detail.tracks.isEmpty {
                Section("Recordings") {
                    ForEach(detail.tracks, id: \.trackId) { track in
                        TwoLineRow(
                            title: track.trackTitle,
                            subtitle: track.albumTitle
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    private func workReleaseMetadata(
        _ release: BridgeWorkReleaseSummary
    ) -> String {
        release.metadataText
    }
}

/// A work-summary row that opens the work when tapped.
struct WorkSummaryButton: View {
    let summary: BridgeWorkSummary
    let openWork: (String) -> Void

    var body: some View {
        Button {
            openWork(summary.workId)
        } label: {
            WorkSummaryRow(summary: summary)
        }
    }
}

private struct WorkSummaryRow: View {
    let summary: BridgeWorkSummary

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: summary.representativeCover, pointSize: ThemeSize.rowArtwork)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            TwoLineRow(title: summary.title, subtitle: summary.composerNames)
        }
        .padding(.vertical, ThemeSpace.inline)
    }
}

#if DEBUG
#Preview {
    NavigationStack {
        WorkDetailScreen(
            workId: "work-1",
            openWork: { _ in },
            openAlbum: { _ in }
        )
    }
    .previewStores()
}
#endif
