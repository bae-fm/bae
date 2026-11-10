import BaeKit
import SwiftUI

/// The composer mode's detail pane: a composer's page (its works and
/// credits, read a page at a time) and a work's detail.
struct ComposerDetailPane: View {
    let paneDetail: ComposerPaneDetail
    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if case .composer(
                    let composerDetail,
                    let rows,
                    let loadedWorkDetail
                ) = paneDetail {
                    BrowseDetailHeader(summary: composerDetail.composer)
                    if let rows {
                        if let error = rows.initialLoadError {
                            Notice(
                                error: error,
                                look: .centered,
                                action: error.retryAction {
                                    Task { await rows.loadInitial() }
                                }
                            )
                        }
                        else {
                            pageRows(rows)
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
                    ContentUnavailableView {
                        Text("Composers")
                    }
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

    /// The composer page's rows in core's order, in one lazy stack since
    /// they are unbounded: each visible row holds the page it sits in. A
    /// heading opens each part of the page, and a gap above a group's
    /// heading work sets the groups apart.
    private func pageRows(_ rows: ComposerDetailRowList) -> some View {
        LazyVStack(alignment: .leading, spacing: 0) {
            ForEach(0..<rows.totalCount, id: \.self) { index in
                pageRow(rows.row(at: index), first: index == 0)
                    .task(id: RowLoadID(epoch: rows.loadEpoch, index: index)) {
                        await rows.holdPage(containing: index)
                    }
            }
        }
    }

    /// One row of the page, or a placeholder of a row's height until its
    /// page answers.
    @ViewBuilder
    private func pageRow(
        _ row: BridgeComposerDetailRow?,
        first: Bool
    ) -> some View {
        if let row {
            VStack(alignment: .leading, spacing: 0) {
                if let heading = row.heading {
                    SectionHeader(title: headingTitle(heading))
                        .padding(.top, first ? 0 : ThemeSpace.section)
                        .padding(.bottom, ThemeSpace.section)
                }
                switch row.item {
                case .work(let work, let part):
                    workRow(work)
                        .padding(.leading, part ? ThemeSpace.edge : 0)
                        .padding(
                            .top,
                            part || row.heading != nil
                                ? ThemeSpace.hairline : ThemeSpace.edge
                        )
                case .releaseCredit(let credit):
                    CreditRow(
                        title: credit.albumTitle,
                        subtitle: credit.sourceCredit
                    )
                    .padding(
                        .top,
                        row.heading != nil ? 0 : ThemeSpace.section
                    )
                case .trackCredit(let credit):
                    CreditRow(
                        title: credit.trackTitle,
                        subtitle: credit.albumTitle
                    )
                    .padding(
                        .top,
                        row.heading != nil ? 0 : ThemeSpace.section
                    )
                }
            }
        }
        else {
            Color.clear.frame(height: ThemeSize.rowArtwork)
        }
    }

    private func headingTitle(_ heading: BridgeComposerHeading) -> String {
        switch heading {
        case .works: String(localized: "Works")
        case .credits: String(localized: "Credits")
        case .recordings: String(localized: "Recordings")
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
            rows: PreviewData.composerDetailRows(),
            work: PreviewData.workDetail
        )
        let uiStore = UiStore()
        let session = PreviewData.browseSession(uiStore: uiStore)
        session.selectComposer("composer-0")
        return ComposerDetailPane(paneDetail: paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }

    #Preview("Composer \u{2014} Many works") {
        let paneDetail = ComposerPaneDetail.composer(
            PreviewData.composerDetail,
            rows: PreviewData.manyComposerDetailRows(workCount: 2_000),
            work: nil
        )
        let uiStore = UiStore()
        let session = PreviewData.browseSession(uiStore: uiStore)
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
        let session = PreviewData.browseSession(uiStore: uiStore)
        return ComposerDetailPane(paneDetail: paneDetail)
            .frame(width: 620, height: 720)
            .environment(session)
            .environment(uiStore)
            .environment(ImageStore.stub())
    }
#endif
