import BaeKit
import SwiftUI

/// A composer's works, credits, and recordings, loaded on demand by artist id.
struct ComposerDetailScreen: View {
    let artistId: String
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    @Environment(LibraryProjectionStore.self)
    private var libraryProjections

    private var detail: BridgeComposerDetail? {
        libraryProjections.composer.value
    }
    private var error: String? {
        libraryProjections.composer.error?.line
            ?? (libraryProjections.composer.delivered && detail == nil
                ? String(localized: "Composer detail not found") : nil)
    }

    var body: some View {
        Group {
            if let detail {
                ComposerDetailContent(
                    detail: detail,
                    openWork: openWork,
                    openAlbum: openAlbum
                )
                .overlay(alignment: .top) {
                    if let error {
                        Text(error).foregroundStyle(.red).padding(12)
                    }
                }
            }
            else if let error {
                Text(error).foregroundStyle(.red).padding(32)
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { libraryProjections.activateComposer(artistId) }
        .onDisappear { libraryProjections.deactivateComposer(artistId) }
    }

    private var navigationTitle: String {
        if let detail {
            return detail.composer.name
        }
        if error != nil {
            return String(localized: "Composers")
        }
        return ""
    }

}

private struct ComposerDetailContent: View {
    let detail: BridgeComposerDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        List {
            Section {
                ComposerSummaryRow(summary: detail.composer)
            }
            if !detail.workGroups.isEmpty {
                Section("Works") {
                    ForEach(detail.workGroups, id: \.id) { group in
                        if let parent = group.parent {
                            WorkSummaryButton(summary: parent, openWork: openWork)
                        }
                        ForEach(group.works, id: \.workId) { work in
                            WorkSummaryButton(summary: work, openWork: openWork)
                        }
                    }
                }
            }
            if !detail.unlinkedReleaseRoles.isEmpty {
                Section("Credits") {
                    ForEach(detail.unlinkedReleaseRoles, id: \.releaseId) {
                        role in
                        Button {
                            openAlbum(role.albumId, role.releaseId)
                        } label: {
                            TwoLineRow(
                                title: role.albumTitle,
                                subtitle: role.sourceCredit
                            )
                        }
                    }
                }
            }
            if !detail.unlinkedTrackRoles.isEmpty {
                Section("Recordings") {
                    ForEach(detail.unlinkedTrackRoles, id: \.trackId) { role in
                        TwoLineRow(
                            title: role.trackTitle,
                            subtitle: role.albumTitle
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

#if DEBUG
#Preview {
    NavigationStack {
        ComposerDetailScreen(
            artistId: "composer-1",
            openWork: { _ in },
            openAlbum: { _, _ in }
        )
    }
    .previewStores()
}
#endif
