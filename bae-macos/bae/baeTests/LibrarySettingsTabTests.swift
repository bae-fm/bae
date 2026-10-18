import Testing

@testable import bae

@MainActor
@Suite("LibraryRemovalConfirmation.message")
struct LibrarySettingsTabForgetMessageTests {

    @Test("synced and never-synced messages are distinct and non-empty")
    func variantsDiffer() {
        let synced = LibraryRemovalConfirmation.message(hasCloudHome: true)
        let local = LibraryRemovalConfirmation.message(hasCloudHome: false)
        #expect(!synced.isEmpty)
        #expect(!local.isEmpty)
        #expect(synced != local)
    }
}
