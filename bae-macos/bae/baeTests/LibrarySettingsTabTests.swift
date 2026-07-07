import Testing

@testable import bae

@MainActor
@Suite("LibrarySettingsTab.forgetConfirmationMessage")
struct LibrarySettingsTabForgetMessageTests {

    @Test("synced and never-synced messages are distinct and non-empty")
    func variantsDiffer() {
        let synced = LibrarySettingsTab.forgetConfirmationMessage(
            hasCloudHome: true,
            hasPendingCloudWork: false
        )
        let local = LibrarySettingsTab.forgetConfirmationMessage(
            hasCloudHome: false,
            hasPendingCloudWork: false
        )
        #expect(!synced.isEmpty)
        #expect(!local.isEmpty)
        #expect(synced != local)
    }

    @Test("pending cloud work appends to the synced base")
    func pendingAppendsToSyncedBase() {
        let base = LibrarySettingsTab.forgetConfirmationMessage(
            hasCloudHome: true,
            hasPendingCloudWork: false
        )
        let withPending = LibrarySettingsTab.forgetConfirmationMessage(
            hasCloudHome: true,
            hasPendingCloudWork: true
        )
        #expect(withPending.hasPrefix(base))
        #expect(withPending.count > base.count)
    }

    @Test("the pending-work flag is ignored without a cloud home")
    func pendingIgnoredWithoutCloudHome() {
        let withoutPending = LibrarySettingsTab.forgetConfirmationMessage(
            hasCloudHome: false,
            hasPendingCloudWork: false
        )
        let withPending = LibrarySettingsTab.forgetConfirmationMessage(
            hasCloudHome: false,
            hasPendingCloudWork: true
        )
        #expect(withoutPending == withPending)
    }
}
