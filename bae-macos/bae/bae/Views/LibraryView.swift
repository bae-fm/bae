import SwiftUI

private let composerLoadBatchSize = 50

private enum ComposerPaneSelection: Equatable {
    case none
    case composer(artistId: String, workId: String?)
    case work(workId: String)

    var composerId: String? {
        if case .composer(let artistId, _) = self {
            return artistId
        }
        return nil
    }

    var workId: String? {
        switch self {
        case .none:
            return nil
        case .composer(_, let workId):
            return workId
        case .work(let workId):
            return workId
        }
    }
}

private enum ComposerPaneDetail {
    case empty
    case composer(BridgeComposerDetail, work: BridgeWorkDetail?)
    case work(BridgeWorkDetail)
}

struct LibraryView: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(LibraryStore.self)
    var libraryStore
    @Environment(UiStore.self)
    var uiStore

    @State
    private var albumList: AlbumList?
    @State
    private var composerList: ComposerList?
    @State
    private var sortCriteria: [BridgeSortCriterion] = Self.loadSortCriteria()
    @State
    private var composerSortCriterion =
        BridgeComposerSortCriterion(field: .name, direction: .ascending)
    @State
    private var composerPaneDetail: ComposerPaneDetail = .empty
    @State
    private var detailSelection: ComposerPaneSelection = .none

    var body: some View {
        Group {
            switch uiStore.libraryBrowserMode {
            case .albums:
                albumContent
            case .composers:
                composerContent
            }
        }
        .background(Theme.background)
        .task(id: sortCriteria) {
            Self.saveSortCriteria(sortCriteria)
            await reloadAlbumList(sort: sortCriteria)
        }
        .task(id: uiStore.libraryBrowserMode) {
            if uiStore.libraryBrowserMode == .composers {
                await ensureComposerListLoaded()
            }
        }
        .task(id: detailSelection.composerId) {
            await loadComposerDetail()
        }
        .task(id: detailSelection) {
            await loadWorkDetail()
        }
        .task(id: composerSortCriterion) {
            if uiStore.libraryBrowserMode == .composers {
                await reloadComposerList(sort: composerSortCriterion)
            }
        }
        .onReceive(libraryStore.libraryShapeSubject) { change in
            switch change {
            case .albumAdded, .albumUpdated, .albumRemoved:
                albumList?.invalidate()
                composerList?.invalidate()
            case .releaseAdded, .releaseUpdated, .releaseRemoved:
                composerList?.invalidate()
            }
        }
        .onReceive(uiStore.libraryNavigationSubject) { target in
            switch target {
            case .composer(let artistId):
                detailSelection = .composer(artistId: artistId, workId: nil)
                composerPaneDetail = .empty
            case .work(let workId):
                detailSelection = .work(workId: workId)
                composerPaneDetail = .empty
            }
        }
    }
}

extension LibraryView {
    private var albumContent: some View {
        Group {
            if let albumList {
                if albumList.totalCount == 0 {
                    ContentUnavailableView(
                        "No albums",
                        systemImage: "square.stack",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    AlbumGridView(
                        list: albumList,
                        sortCriteria: $sortCriteria,
                        availableFields: BridgeSortField.allCases,
                        onPlay: { releaseId in
                            playback.playRelease(releaseId, nil, false)
                        },
                        onAddToQueue: { releaseId in
                            queue.addReleaseToQueue(releaseId)
                        },
                        onAddNext: { releaseId in
                            queue.addReleaseNext(releaseId)
                        },
                        onShuffleLibrary: { playback.playLibraryShuffled() },
                        headerTitle: String(localized: "Library"),
                    ) { albumId in
                        AlbumDetailView(albumId: albumId)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private var composerContent: some View {
        Group {
            if let composerList {
                if composerList.totalCount == 0 {
                    ContentUnavailableView(
                        "No composers",
                        systemImage: "person.wave.2",
                        description: Text("Import some music to get started"),
                    )
                }
                else {
                    HSplitView {
                        composerListView(composerList)
                            .frame(minWidth: 260, idealWidth: 320)
                        composerDetailView
                            .frame(minWidth: 420)
                    }
                }
            }
            else {
                ProgressView()
            }
        }
    }

    private func composerListView(_ list: ComposerList) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            composerHeader
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            List {
                ForEach(0..<list.totalCount, id: \.self) { index in
                    composerRow(at: index, list: list)
                        .task(
                            id: RowLoadID(
                                epoch: list.loadEpoch,
                                index: index
                            )
                        ) {
                            let first = max(
                                0,
                                index - composerLoadBatchSize / 2
                            )
                            let end = min(
                                first + composerLoadBatchSize,
                                list.totalCount
                            )
                            await list.loadRange(
                                offset: first,
                                limit: end - first
                            )
                        }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.background)
        }
    }

    private var composerHeader: some View {
        HStack(alignment: .firstTextBaseline) {
            Text("Composers")
                .font(.title.bold())
            Spacer()
            LibraryModeMenu()
            composerSortControls
        }
    }

    private var composerSortControls: some View {
        HStack(spacing: 8) {
            Menu {
                ForEach(BridgeComposerSortField.allCases, id: \.self) { field in
                    Button(field.displayName) {
                        composerSortCriterion = BridgeComposerSortCriterion(
                            field: field,
                            direction: composerSortCriterion.direction
                        )
                    }
                }
            } label: {
                Text(composerSortCriterion.field.displayName)
            }
            .menuStyle(.borderlessButton)
            Button {
                let nextDirection: BridgeSortDirection =
                    composerSortCriterion.direction == .ascending
                    ? .descending : .ascending
                composerSortCriterion = BridgeComposerSortCriterion(
                    field: composerSortCriterion.field,
                    direction: nextDirection
                )
            } label: {
                Image(
                    systemName: composerSortCriterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
            .buttonStyle(.borderless)
            .help(
                composerSortCriterion.direction == .ascending
                    ? String(localized: "Sort Descending")
                    : String(localized: "Sort Ascending")
            )
        }
    }

    private func composerRow(at index: Int, list: ComposerList) -> some View {
        let id = list.idAt(index)
        return ComposerListRow(
            id: id,
            isSelected: id != nil && detailSelection.composerId == id,
            select: { id in
                detailSelection = .composer(artistId: id, workId: nil)
                composerPaneDetail = .empty
            }
        )
    }

    private var composerDetailView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if case .composer(let composerDetail, let loadedWorkDetail) =
                    composerPaneDetail
                {
                    ComposerDetailHeader(summary: composerDetail.composer)
                    if !composerDetail.workGroups.isEmpty {
                        SectionHeader(title: String(localized: "Works"))
                        ForEach(composerDetail.workGroups) { group in
                            ComposerWorkGroupView(group: group) { workId in
                                if let artistId = detailSelection.composerId {
                                    detailSelection = .composer(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    composerPaneDetail = .composer(
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
                        Divider()
                        WorkDetailView(
                            detail: loadedWorkDetail,
                            openWork: { workId in
                                if case .composer(let artistId, _) =
                                    detailSelection
                                {
                                    detailSelection = .composer(
                                        artistId: artistId,
                                        workId: workId
                                    )
                                    composerPaneDetail = .composer(
                                        composerDetail,
                                        work: nil
                                    )
                                }
                            },
                            openAlbum: { albumId, releaseId in
                                uiStore.selectRelease(
                                    releaseId,
                                    inAlbum: albumId
                                )
                                uiStore.selectAlbum(albumId)
                                uiStore.setLibraryBrowserMode(.albums)
                            }
                        )
                    }
                }
                if case .work(let loadedWorkDetail) = composerPaneDetail {
                    WorkDetailView(
                        detail: loadedWorkDetail,
                        openWork: { workId in
                            detailSelection = .work(workId: workId)
                            composerPaneDetail = .empty
                        },
                        openAlbum: { albumId, releaseId in
                            uiStore.selectRelease(releaseId, inAlbum: albumId)
                            uiStore.selectAlbum(albumId)
                            uiStore.setLibraryBrowserMode(.albums)
                        }
                    )
                }
                if detailSelection == .none {
                    ContentUnavailableView(
                        "Composers",
                        systemImage: "person.wave.2"
                    )
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Theme.surface)
    }

    private func ensureComposerListLoaded() async {
        guard composerList == nil else {
            return
        }
        let newList = makeComposerList(sort: composerSortCriterion)
        await newList.loadInitial()
        composerList = newList
    }

    private func loadComposerDetail() async {
        guard let selectedComposerId = detailSelection.composerId else {
            return
        }
        do {
            let getComposerDetail = library.getComposerDetail
            let detail =
                try await DetachedWork.run {
                    try getComposerDetail(selectedComposerId)
                }
            guard !Task.isCancelled else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Composer detail not found")
                    )
                )
                return
            }
            composerPaneDetail = .composer(detail, work: nil)
            if case .composer(let artistId, nil) = detailSelection,
                artistId == selectedComposerId,
                let defaultWorkId = detail.defaultWorkId
            {
                detailSelection = .composer(
                    artistId: selectedComposerId,
                    workId: defaultWorkId
                )
            }
        }
        catch {
            uiStore.showError(DisplayError(line: error.localizedDescription))
        }
    }

    private func loadWorkDetail() async {
        let selectedDetail = detailSelection
        guard let selectedWorkId = selectedDetail.workId else {
            return
        }
        do {
            let getWorkDetail = library.getWorkDetail
            let detail =
                try await DetachedWork.run {
                    try getWorkDetail(selectedWorkId)
                }
            guard !Task.isCancelled else {
                return
            }
            guard detailSelection == selectedDetail else {
                return
            }
            guard let detail else {
                uiStore.showError(
                    DisplayError(
                        line: String(localized: "Work detail not found")
                    )
                )
                return
            }
            switch selectedDetail {
            case .composer(let artistId, .some):
                guard
                    case .composer(let composerDetail, _) = composerPaneDetail,
                    composerDetail.composer.artistId == artistId
                else {
                    uiStore.showError(
                        DisplayError(
                            line: String(localized: "Composer detail not found")
                        )
                    )
                    return
                }
                composerPaneDetail = .composer(composerDetail, work: detail)
            case .work:
                composerPaneDetail = .work(detail)
            case .none, .composer(_, .none):
                return
            }
        }
        catch {
            uiStore.showError(DisplayError(line: error.localizedDescription))
        }
    }

    private func makeAlbumList(sort: [BridgeSortCriterion]) -> AlbumList {
        AlbumList(
            pageSource: LibraryAlbumPageSource(
                library: library,
                sort: sort
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row)
                }
            },
            onError: { [uiStore] error in
                uiStore.showError(error)
            },
        )
    }

    private func makeComposerList(
        sort: BridgeComposerSortCriterion
    ) -> ComposerList {
        ComposerList(
            pageSource: LibraryComposerPageSource(
                library: library,
                sort: sort
            ),
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internComposerSummary(row)
                }
            },
            onError: { [uiStore] error in
                uiStore.showError(error)
            },
        )
    }

    private func reloadComposerList(sort: BridgeComposerSortCriterion) async {
        let newList = makeComposerList(sort: sort)
        await newList.loadInitial()
        composerList = newList
    }

    private func reloadAlbumList(sort: [BridgeSortCriterion]) async {
        let newList = makeAlbumList(sort: sort)
        await newList.loadInitial()
        albumList = newList
    }

    private static let sortCriteriaKey = "librarySortCriteria"

    private static func loadSortCriteria() -> [BridgeSortCriterion] {
        guard let data = UserDefaults.standard.data(forKey: sortCriteriaKey),
            let criteria = [BridgeSortCriterion].fromJSON(data),
            !criteria.isEmpty
        else {
            return [
                BridgeSortCriterion(field: .dateAdded, direction: .descending)
            ]
        }
        return criteria
    }

    private static func saveSortCriteria(_ criteria: [BridgeSortCriterion]) {
        if let data = criteria.toJSON() {
            UserDefaults.standard.set(data, forKey: sortCriteriaKey)
        }
    }

}

extension BridgeComposerWorkGroup: Identifiable {}

private struct ComposerListRow: View {
    @Environment(LibraryStore.self)
    private var libraryStore

    let id: String?
    let isSelected: Bool
    let select: (String) -> Void

    var body: some View {
        let summary = id.flatMap { libraryStore.composerSummaries[$0] }
        Button(action: {
            guard let id else {
                return
            }
            select(id)
        }) {
            ZStack(alignment: .leading) {
                ComposerSummaryPlaceholder()
                    .opacity(summary == nil ? 1 : 0)
                    .allowsHitTesting(summary == nil)
                ComposerSummaryRow(summary: summary, isSelected: isSelected)
                    .opacity(summary == nil ? 0 : 1)
                    .allowsHitTesting(summary != nil)
            }
        }
        .buttonStyle(.plain)
        .disabled(summary == nil)
    }
}

private struct ComposerSummaryRow: View {
    let summary: BridgeComposerSummary?
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary?.image, pointSize: 36)
                .frame(width: 36, height: 36)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                OptionalLineText(
                    text: summary?.name,
                    font: .body,
                    foreground: .primary
                )
                OptionalLineText(
                    text: workCountText,
                    font: .caption,
                    foreground: .secondary
                )
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isSelected ? Color.accentColor.opacity(0.18) : .clear)
        )
    }

    private var workCountText: String? {
        guard let summary else {
            return nil
        }
        return "\(summary.workCount) \(String(localized: "Works"))"
    }
}

private struct OptionalLineText: View {
    let text: String?
    let font: Font
    let foreground: StableOptionalTextForeground

    var body: some View {
        StableOptionalText(
            text: text,
            font: font,
            foreground: foreground,
            lineHeight: lineHeight,
            lineLimit: 1
        )
    }

    private var lineHeight: CGFloat {
        switch font {
        case .caption:
            12
        default:
            16
        }
    }
}

private struct ComposerSummaryPlaceholder: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 6)
                .fill(.secondary.opacity(0.15))
                .frame(width: 36, height: 36)
            VStack(alignment: .leading, spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 140, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 80, height: 10)
            }
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerDetailHeader: View {
    let summary: BridgeComposerSummary

    var body: some View {
        HStack(spacing: 16) {
            ImageView(imageRef: summary.image, pointSize: 72)
                .frame(width: 72, height: 72)
                .clipShape(RoundedRectangle(cornerRadius: 8))
            VStack(alignment: .leading, spacing: 6) {
                Text(summary.name)
                    .font(.title.bold())
                    .lineLimit(2)
                Text("\(summary.workCount) \(String(localized: "Works"))")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct WorkSummaryRow: View {
    let summary: BridgeWorkSummary

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: summary.representativeCover, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            VStack(alignment: .leading, spacing: 3) {
                Text(summary.title)
                    .font(.body)
                    .lineLimit(1)
                OptionalLineText(
                    text: summary.composerNames,
                    font: .caption,
                    foreground: .secondary
                )
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct ComposerWorkGroupView: View {
    let group: BridgeComposerWorkGroup
    let openWork: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(parentRows, id: \.workId) { parent in
                Button(action: { openWork(parent.workId) }) {
                    WorkSummaryRow(summary: parent)
                }
                .buttonStyle(.plain)
            }
            VStack(alignment: .leading, spacing: 6) {
                ForEach(group.works, id: \.workId) { work in
                    Button(action: { openWork(work.workId) }) {
                        WorkSummaryRow(summary: work)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.leading, group.parent == nil ? 0 : 18)
        }
    }

    private var parentRows: [BridgeWorkSummary] {
        guard let parent = group.parent else {
            return []
        }
        return [parent]
    }
}

private struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.headline)
            .padding(.top, 4)
    }
}

private struct CreditRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.body)
                .lineLimit(1)
            StableOptionalText(
                text: subtitle,
                font: .caption,
                foreground: .secondary,
                lineHeight: 12,
                lineLimit: 1
            )
        }
        .padding(.vertical, 4)
    }
}

private struct WorkDetailView: View {
    let detail: BridgeWorkDetail
    let openWork: (String) -> Void
    let openAlbum: (String, String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(detail.work.title)
                .font(.title2.bold())
                .lineLimit(2)
            if !detail.childWorks.isEmpty {
                SectionHeader(title: String(localized: "Works"))
                ForEach(detail.childWorks, id: \.workId) { work in
                    Button(action: { openWork(work.workId) }) {
                        WorkSummaryRow(summary: work)
                    }
                    .buttonStyle(.plain)
                }
            }
            if !detail.releases.isEmpty {
                SectionHeader(title: String(localized: "Releases"))
                ForEach(detail.releases, id: \.releaseId) { release in
                    Button(action: {
                        openAlbum(release.albumId, release.releaseId)
                    }) {
                        HStack(spacing: 12) {
                            ImageView(imageRef: release.cover, pointSize: 42)
                                .frame(width: 42, height: 42)
                                .clipShape(RoundedRectangle(cornerRadius: 6))
                            VStack(alignment: .leading, spacing: 3) {
                                Text(release.albumTitle)
                                    .font(.body)
                                    .lineLimit(1)
                                StableOptionalText(
                                    text: workReleaseMetadata(release),
                                    font: .caption,
                                    foreground: .secondary,
                                    lineHeight: 12,
                                    lineLimit: 1
                                )
                            }
                            Spacer()
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
            if !detail.tracks.isEmpty {
                SectionHeader(title: String(localized: "Recordings"))
                ForEach(detail.tracks, id: \.trackId) { track in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(track.trackTitle)
                            .font(.body)
                            .lineLimit(1)
                        Text(track.albumTitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
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
