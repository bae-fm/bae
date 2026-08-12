import Testing

@testable import bae

@MainActor
@Suite("LibraryRemovalConfirmation.message")
struct LibrarySettingsTabForgetMessageTests {

    @Test("synced and never-synced messages are distinct and non-empty")
    func variantsDiffer() {
        let synced = LibraryRemovalConfirmation.message(
            hasCloudHome: true,
            hasPendingCloudWork: false
        )
        let local = LibraryRemovalConfirmation.message(
            hasCloudHome: false,
            hasPendingCloudWork: false
        )
        #expect(!synced.isEmpty)
        #expect(!local.isEmpty)
        #expect(synced != local)
    }

    @Test("pending cloud work appends to the synced base")
    func pendingAppendsToSyncedBase() {
        let base = LibraryRemovalConfirmation.message(
            hasCloudHome: true,
            hasPendingCloudWork: false
        )
        let withPending = LibraryRemovalConfirmation.message(
            hasCloudHome: true,
            hasPendingCloudWork: true
        )
        #expect(withPending.hasPrefix(base))
        #expect(withPending.count > base.count)
    }

    @Test("the pending-work flag is ignored without a cloud home")
    func pendingIgnoredWithoutCloudHome() {
        let withoutPending = LibraryRemovalConfirmation.message(
            hasCloudHome: false,
            hasPendingCloudWork: false
        )
        let withPending = LibraryRemovalConfirmation.message(
            hasCloudHome: false,
            hasPendingCloudWork: true
        )
        #expect(withoutPending == withPending)
    }
}
