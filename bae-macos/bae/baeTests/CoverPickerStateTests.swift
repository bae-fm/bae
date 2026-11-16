import BaeKit
import Testing

@testable import bae

@MainActor
struct CoverPickerStateTests {
    @Test("An unsuccessful cover save does not dismiss the picker")
    func failedSave() async throws {
        let state = CoverPickerState()
        var dismissed = false
        state.save(
            { throw StubError.notImplemented },
            onSaved: {
                dismissed = true
                Wait.happened()
            }
        )
        try await Wait.until { !state.isSaving }
        #expect(!state.isSaving)
        #expect(!dismissed)
        #expect(state.error != nil)
        state.save(
            {},
            onSaved: {
                dismissed = true
                Wait.happened()
            }
        )
        try await Wait.until { !state.isSaving }
        #expect(dismissed)
        #expect(state.error == nil)
    }
}
