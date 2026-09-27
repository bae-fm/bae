import BaeKit
import SwiftUI

/// The composer mode's detail pane: a composer's works and credits, in lazy
/// stacks since they are unbounded, and a work's detail.
struct ComposerDetailPane: View {
    let paneDetail: ComposerPaneDetail
    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if case .composer(let composerDetail, let loadedWorkDetail) =
                    paneDetail
                {
                    BrowseDetailHeader(summary: composerDetail.composer)
                    if !composerDetail.workGroups.isEmpty {
                        SectionHeader(title: String(localized: "Works"))
                        // Every row is a child of this one stack so each row
                        // loads lazily; a gap above a group's first row sets
                        // the groups apart.
                        LazyVStack(
                            alignment: .leading,
                            spacing: ThemeSpace.hairline
                        ) {
                            ForEach(
                                Array(composerDetail.workGroups.enumerated()),
                                id: \.element.id
                            ) { groupIndex, group in
                                let groupGap =
                                    groupIndex == 0 ? 0 : ThemeSpace.edge
                                if let parent = group.parent {
                                    workRow(parent)
                                        .padding(.top, groupGap)
                                }
                                ForEach(
                                    Array(group.works.enumerated()),
                                    id: \.element.workId
                                ) { workIndex, work in
                                    // Without a parent row, the first work
                                    // is the group's first row.
                                    let leadsGroup =
                                        group.parent == nil && workIndex == 0
                                    workRow(work)
                                        .padding(
                                            .leading,
                                            group.parent == nil
                                                ? 0 : ThemeSpace.edge
                                        )
                                        .padding(
                                            .top,
                                            leadsGroup ? groupGap : 0
                                        )
                                }
                            }
                        }
                    }
                    if !composerDetail.unlinkedReleaseRoles.isEmpty {
                        SectionHeader(title: String(localized: "Credits"))
                        LazyVStack(
                            alignment: .leading,
                            spacing: ThemeSpace.section
                        ) {
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
                    }
                    if !composerDetail.unlinkedTrackRoles.isEmpty {
                        LazyVStack(
                            alignment: .leading,
                            spacing: ThemeSpace.section
                        ) {
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
                    }
                    if let loadedWorkDetail {
                        Rectangle()
                            .fill(Theme.hairline)
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
            .padding(ThemeSpace.section)
            .frame(maxWidth: 900, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "composerDetail")
        // One surface step above the master list, with a hairline between.
        .background(Theme.surface)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Theme.hairline)
                .frame(width: 1)
        }
    }

    /// One work in the works list; opens that work in this pane.
    private func workRow(_ work: BridgeWorkSummary) -> some View {
        Button(action: { openWork(work.workId) }) {
            DetailMediaRow(
                image: work.representativeCover,
                title: work.title,
                subtitle: work.composerNames
            )
        }
        .buttonStyle(DetailRowButtonStyle())
    }

    private func openWork(_ workId: String) {
        guard let artistId = session.detailSelection.composerId else {
            return
        }
        session.selectComposerWork(artistId: artistId, workId: workId)
    }
}

#if DEBUG
    #Preview("Composer \u{2014} Detail") {
        let paneDetail = ComposerPaneDetail.composer(
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
        return ComposerDetailPane(paneDetail: paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }

    #Preview("Composer \u{2014} Many works") {
        let paneDetail = ComposerPaneDetail.composer(
            PreviewData.largeComposerDetail(workCount: 2_000),
            work: nil
        )
        let uiStore = UiStore()
        let libraryStore = PreviewData.seededComposerStore()
        let session = PreviewData.browseSession(
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        session.selectComposer("composer-0")
        return ComposerDetailPane(paneDetail: paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }

    #Preview("Composer \u{2014} Empty") {
        let paneDetail = ComposerPaneDetail.empty
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let session = PreviewData.browseSession(
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        return ComposerDetailPane(paneDetail: paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }
#endif
