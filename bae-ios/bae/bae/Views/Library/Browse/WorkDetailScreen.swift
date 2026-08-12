import BaeKit
import SwiftUI

/// A work's child works, releases, and recordings, loaded on demand by work id.
struct WorkDetailScreen: View {
    let workId: String
    let openWork: (String) -> Void
    let openAlbum: (BridgeWorkReleaseSummary) -> Void

    @Environment(Library.self)
    private var library

    @State
    private var detail: BridgeWorkDetail?
    @State
    private var error: String?

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
        .task(id: workId) {
            await load()
        }
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

    private func load() async {
        error = nil
        for await result in library.workDetails(workId) {
            guard !Task.isCancelled else { return }
            switch result {
            case .success(let loaded):
                guard let loaded else {
                    error = String(localized: "Work detail not found")
                    continue
                }
                detail = loaded
                error = nil
            case .failure(let bridgeError):
                error = bridgeError.displayLine
            }
        }
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
                            HStack(spacing: 12) {
                                ImageView(imageRef: release.cover, pointSize: 42)
                                    .frame(width: 42, height: 42)
                                    .clipShape(RoundedRectangle(cornerRadius: 6))
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

/// A work-summary row wrapped in a button that opens the work. Shared by the
/// composer and work browse detail lists.
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
        HStack(spacing: 12) {
            ImageView(imageRef: summary.representativeCover, pointSize: 42)
                .frame(width: 42, height: 42)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            TwoLineRow(title: summary.title, subtitle: summary.composerNames)
        }
        .padding(.vertical, 4)
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
