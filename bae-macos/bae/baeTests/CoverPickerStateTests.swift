import BaeKit
import Testing

@testable import bae

@MainActor
struct CoverPickerStateTests {
    @Test("A cancelled request cannot finish a replacement's loading state")
    func replacedLoad() async {
        let state = CoverPickerState()
        let starts = AsyncStream<Void>.makeStream()
        var started = starts.stream.makeAsyncIterator()
        var finishFirst: CheckedContinuation<BridgeRemoteCoverGallery, Never>?
        var finishSecond: CheckedContinuation<BridgeRemoteCoverGallery, Never>?
        let first = Task {
            await state.load {
                await withCheckedContinuation {
                    finishFirst = $0
                    starts.continuation.yield()
                }
            }
        }
        await started.next()
        first.cancel()
        let second = Task {
            await state.load {
                await withCheckedContinuation {
                    finishSecond = $0
                    starts.continuation.yield()
                }
            }
        }
        await started.next()
        finishFirst?.resume(returning: .unlinked)
        await first.value
        #expect(state.remoteItems == .loading([]))
        finishSecond?
            .resume(returning: .linked(covers: PreviewData.remoteCovers))
        await second.value
        #expect(
            state.remoteItems
                == RemoteCoverItems(.linked(covers: PreviewData.remoteCovers))
        )
        starts.continuation.finish()
    }

    @Test(
        "A failed artwork refresh retains prior choices and reports the error"
    )
    func failedRefresh() async {
        let state = CoverPickerState()
        await state.load { .linked(covers: PreviewData.remoteCovers) }
        let covers = state.remoteItems.items
        await state.load { throw StubError.notImplemented }
        #expect(state.remoteItems.items == covers)
        #expect(state.remoteItems.failureMessage != nil)
        #expect(!state.remoteItems.isLoading)
    }

    @Test("Unlinked, empty, and failed lookups remain distinct")
    func lookupStates() async {
        let state = CoverPickerState()
        #expect(state.remoteItems == .loading([]))
        await state.load { .unlinked }
        #expect(state.remoteItems == .unlinked)
        #expect(!state.remoteItems.canRefresh)
        await state.load { .linked(covers: []) }
        #expect(state.remoteItems == .linked([]))
        #expect(state.remoteItems.canRefresh)
        await state.load { throw StubError.notImplemented }
        #expect(state.remoteItems.failureMessage != nil)
        #expect(state.remoteItems != .linked([]))
        #expect(state.remoteItems.canRefresh)
    }

    @Test("An unsuccessful cover save does not dismiss the picker")
    func failedSave() async {
        let state = CoverPickerState()
        var dismissed = false
        state.save(
            { throw StubError.notImplemented },
            onSaved: { dismissed = true }
        )
        for _ in 0..<100 {
            await Task.yield()
            if !state.isSaving { break }
        }
        #expect(!state.isSaving)
        #expect(!dismissed)
        #expect(state.errorMessage != nil)
        state.save({}, onSaved: { dismissed = true })
        for _ in 0..<100 {
            await Task.yield()
            if !state.isSaving { break }
        }
        #expect(dismissed)
        #expect(state.errorMessage == nil)
    }
}
