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
        var finishFirst: CheckedContinuation<[BridgeRemoteCover], Never>?
        var finishSecond: CheckedContinuation<[BridgeRemoteCover], Never>?
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
        finishFirst?.resume(returning: [])
        await first.value
        #expect(state.isLoading)
        #expect(state.remoteCovers == nil)
        finishSecond?.resume(returning: PreviewData.remoteCovers)
        await second.value
        #expect(!state.isLoading)
        #expect(state.remoteCovers == PreviewData.remoteCovers)
        starts.continuation.finish()
    }

    @Test(
        "A failed artwork refresh retains prior choices and reports the error"
    )
    func failedRefresh() async {
        let state = CoverPickerState()
        await state.load { PreviewData.remoteCovers }
        let covers = state.remoteCovers
        await state.load { throw StubError.notImplemented }
        #expect(state.remoteCovers == covers)
        #expect(state.errorMessage != nil)
        #expect(!state.isLoading)
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
