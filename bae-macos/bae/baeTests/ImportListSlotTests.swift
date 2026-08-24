import BaeKit
import Foundation
import Testing

@testable import bae

private struct ReadFailed: Error {}

/// A source whose every page fails immediately.
private struct FailingPageSource: PageSource {
    typealias Row = BridgeImportListItem

    func subscribe(
        offset _: Int,
        limit _: Int,
        onValue _: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let task = Task { @MainActor in onError(ReadFailed()) }
        return TaskBackedSubscription(task: task)
    }

    var pages: ImportListPages {
        ImportListPages(source: self, setView: { _ in })
    }
}

private final class TaskBackedSubscription: PageSubscription,
    @unchecked Sendable
{
    private let task: Task<Void, Never>

    init(task: Task<Void, Never>) { self.task = task }

    func cancel() { task.cancel() }
}

@MainActor
private func waitUntil(_ predicate: @MainActor () -> Bool) async {
    for _ in 0..<500 {
        if predicate() { return }
        await Task.yield()
    }
}

/// The import tab decides between three panes — the list, the "add a folder"
/// prompt, and the read failure — before one is drawn, so it has to know that
/// its first page read failed.
///
/// `PaginatedList` does not hand a first-page failure to `onError`: it keeps it
/// as `initialLoadError` for a list view to render inline, which is where every
/// other list surface reads it. The import tab read only `onError`, so the one
/// failure that matters at launch went nowhere and a library nobody could look
/// at rendered as a library with no folders.
@MainActor
@Suite("Import list slot read failures")
struct ImportListSlotTests {
    @Test("a failed first page becomes the slot's failure and an alert")
    func aFailedFirstPageIsSurfaced() async {
        let uiStore = UiStore()
        let slot = ImportListSlot(
            importStore: ImportStore(),
            uiStore: uiStore,
            makeSource: { _ in FailingPageSource().pages }
        )

        #expect(slot.loadFailure == nil)
        #expect(uiStore.lastError == nil)

        slot.startLoad()
        await waitUntil { slot.loadFailure != nil }

        #expect(slot.loadFailure != nil)
        // The same failure is raised as the global alert, the way every other
        // background failure reaches the person.
        #expect(uiStore.lastError != nil)
    }
}
