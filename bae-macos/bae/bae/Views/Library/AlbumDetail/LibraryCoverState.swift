import BaeKit
import SwiftUI

/// What a library release's cover picker shows: the gallery core reads live
/// for the chosen source, and where the lookup of the catalogs' covers
/// stands. A lookup stores its covers in core, and a change to the release's
/// files or saved cover is a write, so each reaches the open picker as a new
/// value; the filter is the request, changed in place.
@MainActor
@Observable
final class LibraryCoverState {
    var filter = ArtworkBrowserState.Filter.all {
        didSet {
            guard filter != oldValue else { return }
            show()
        }
    }
    private(set) var content = CoverGalleryContent.empty
    private(set) var isRefreshing = true
    /// Why the last lookup of the catalogs' covers failed.
    private(set) var failure: DisplayError?
    /// Why the live read failed.
    private(set) var readFailure: DisplayError?
    private var fetch = BridgeCoverFetch.notFetched
    private var releaseId: String?
    private var query: CoverGalleryQuery?
    private var values:
        ScreenSubscription<PulledValues<BridgeCoverGallerySnapshot>>?
    private var refreshTask: Task<Void, Never>?
    private let refreshCovers: () async throws -> Void

    init(refresh: @escaping () async throws -> Void) {
        refreshCovers = refresh
    }

    /// The lookup the gallery shows beside its covers.
    var lookup: RemoteCoverItems {
        let items = content.remote?.items ?? []
        if isRefreshing { return .loading(items) }
        if let failure { return .failed(items, failure: failure) }
        if fetch == .unlinked { return .unlinked }
        return .linked(items)
    }

    /// Follow `releaseId`'s picker, then look the catalogs up again.
    func open(releaseId: String, query: CoverGalleryQuery) async {
        self.releaseId = releaseId
        if self.query == nil {
            self.query = query
            values = ScreenSubscription(
                query.values,
                onValue: { [weak self] in self?.receive($0) },
                onError: { [weak self] in
                    self?.readFailure = DisplayError($0)
                }
            )
        }
        show()
        await refresh()
    }

    func startRefresh() {
        refreshTask?.cancel()
        refreshTask = Task { await refresh() }
    }

    func cancel() {
        refreshTask?.cancel()
        values?.stop()
        values = nil
        query = nil
    }

    private func show() {
        guard let releaseId, let query else { return }
        do {
            try query.setRequest(releaseId, filter.bridge)
        }
        catch {
            readFailure = DisplayError(error)
        }
    }

    /// Take a value only while it answers the request shown now.
    private func receive(_ snapshot: BridgeCoverGallerySnapshot) {
        guard let request = snapshot.request, request.releaseId == releaseId,
            request.filter == filter.bridge, let gallery = snapshot.gallery
        else { return }
        content = CoverGalleryContent(gallery)
        fetch = gallery.fetch
        readFailure = nil
    }

    private func refresh() async {
        isRefreshing = true
        failure = nil
        do {
            try await refreshCovers()
            try Task.checkCancellation()
        }
        catch is CancellationError { return }
        catch {
            failure = DisplayError(error)?
                .rewording {
                    String(localized: "Failed to load covers: \($0)")
                }
        }
        isRefreshing = false
    }
}
