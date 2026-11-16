import BaeKit
import SwiftUI

/// Holds the gallery that core's live query returns for a library release's
/// or a candidate's picker under the chosen source, and the status of the
/// catalog cover lookup. A lookup, or a change to the covers the picker can
/// show, writes the library, so the query delivers a new value. Changing
/// `filter` or `focusedCover` sets the query's request, and the query says
/// which cover it focuses and where that cover sits.
@MainActor
@Observable
final class CoverGalleryState {
    var filter = ArtworkBrowserState.Filter.all {
        didSet {
            guard filter != oldValue else { return }
            show()
        }
    }
    /// The cover the person focused last; none until they focus one, when
    /// the query focuses the owner's own cover.
    var focusedCover: BridgeCoverKey? {
        didSet {
            guard focusedCover != oldValue else { return }
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
    /// Whose covers the picker shows.
    private let owner: BridgeCoverGalleryOwner
    private var query: CoverGalleryQuery?
    private var values:
        ScreenSubscription<PulledValues<BridgeCoverGallerySnapshot>>?
    private var refreshTask: Task<Void, Never>?
    private let refreshCovers: () async throws -> Void

    init(
        owner: BridgeCoverGalleryOwner,
        refresh: @escaping () async throws -> Void
    ) {
        self.owner = owner
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

    /// Follow the owner's picker, then look the catalogs up again.
    func open(query: CoverGalleryQuery) async {
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
        guard let query else { return }
        do {
            try query.setRequest(owner, filter.bridge, focusedCover)
        }
        catch {
            readFailure = DisplayError(error)
        }
    }

    /// Take a value only while it answers the request shown now.
    private func receive(_ snapshot: BridgeCoverGallerySnapshot) {
        guard let request = snapshot.request, request.owner == owner,
            request.filter == filter.bridge, request.focus == focusedCover,
            let gallery = snapshot.gallery
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
