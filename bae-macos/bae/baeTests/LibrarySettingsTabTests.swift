import AppKit
import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("LibraryRemovalConfirmation.message")
struct LibrarySettingsTabForgetMessageTests {
    @Test("synced and never-synced messages are distinct and non-empty")
    func variantsDiffer() {
        let synced = LibraryRemovalConfirmation.message(syncs: true)
        let local = LibraryRemovalConfirmation.message(syncs: false)
        #expect(!synced.isEmpty)
        #expect(!local.isEmpty)
        #expect(synced != local)
    }
}
