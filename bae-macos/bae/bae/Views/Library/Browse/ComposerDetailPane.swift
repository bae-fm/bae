import BaeKit
import SwiftUI

/// The composer browse mode's detail pane: a composer's works, credits, and
/// recordings, plus an inline or standalone work detail. Renders whatever
/// `paneDetail` holds and drives selection through the session; the parent's
/// `.task` loaders write `paneDetail` back as detail payloads arrive.
struct ComposerDetailPane: View {
    @Binding
    var paneDetail: ComposerPaneDetail
    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if case .composer(let composerDetail, let loadedWorkDetail) =
                    paneDetail
                {
                    BrowseDetailHeader(summary: composerDetail.composer)
                    if !composerDetail.workGroups.isEmpty {
                        SectionHeader(title: String(localized: "Works"))
                        ForEach(composerDetail.workGroups) { group in
                            ComposerWorkGroupView(group: group) { workId in
                                if let artistId = session.detailSelection
                                    .composerId
                                {
                                    session.selectComposerWork(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    paneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            }
                        }
                    }
                    if !composerDetail.unlinkedReleaseRoles.isEmpty {
                        SectionHeader(title: String(localized: "Credits"))
                        ForEach(
                            composerDetail.unlinkedReleaseRoles,
                            id: \.releaseId
                        ) { role in
                            CreditRow(
                                title: role.albumTitle,
                                subtitle: role.sourceCredit
                            )
                        }
                    }
                    if !composerDetail.unlinkedTrackRoles.isEmpty {
                        ForEach(
                            composerDetail.unlinkedTrackRoles,
                            id: \.trackId
                        ) { role in
                            CreditRow(
                                title: role.trackTitle,
                                subtitle: role.albumTitle
                            )
                        }
                    }
                    if let loadedWorkDetail {
                        Rectangle()
                            .fill(Color.primary.opacity(0.08))
                            .frame(height: 1)
                        WorkDetailView(
                            detail: loadedWorkDetail,
                            openWork: { workId in
                                if case .composer(let artistId, _) =
                                    session.detailSelection
                                {
                                    session.selectComposerWork(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    paneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            },
                            openAlbum: { albumId, releaseId in
                                uiStore.navigateToAlbum(
                                    albumId,
                                    releaseId: releaseId
                                )
                            }
                        )
                    }
                }
                if case .work(let loadedWorkDetail) = paneDetail {
                    WorkDetailView(
                        detail: loadedWorkDetail,
                        openWork: { workId in
                            session.selectWork(workId)
                            paneDetail = .empty
                        },
                        openAlbum: { albumId, releaseId in
                            uiStore.navigateToAlbum(
                                albumId,
                                releaseId: releaseId
                            )
                        }
                    )
                }
                if session.detailSelection == .none {
                    ContentUnavailableView(
                        "Composers",
                        systemImage: "person.wave.2"
                    )
                }
            }
            .padding(24)
            .frame(maxWidth: 900, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "composerDetail")
        // The detail pane sits one surface step above the master list, with a
        // hairline on its leading edge separating it from the base-background
        // list.
        .background(Theme.surface)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Color.primary.opacity(0.08))
                .frame(width: 1)
        }
    }
}

#if DEBUG
    #Preview("Composer \u{2014} Detail") {
        @Previewable
        @State
        var paneDetail = ComposerPaneDetail.composer(
            PreviewData.composerDetail,
            work: PreviewData.workDetail
        )
        let uiStore = UiStore()
        let libraryStore = PreviewData.seededComposerStore()
        let session = PreviewData.browseSession(
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        session.selectComposer("composer-0")
        return ComposerDetailPane(paneDetail: $paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(MediaPaths.stub)
    }

    #Preview("Composer \u{2014} Empty") {
        @Previewable
        @State
        var paneDetail = ComposerPaneDetail.empty
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let session = PreviewData.browseSession(
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return ComposerDetailPane(paneDetail: $paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(MediaPaths.stub)
    }
#endif
