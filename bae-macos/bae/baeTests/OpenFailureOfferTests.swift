import BaeKit
import Testing

@testable import bae

/// What a library that failed to open offers: Reset from cloud when core
/// failed the open as one a reset from the cloud loads the library again
/// from, nothing to reset from otherwise.
struct OpenFailureOfferTests {
    private let damaged = BridgeError.Diagnostic(
        category: .databaseDamaged,
        detail: "x"
    )

    private func openFailure(_ error: BridgeOpenError) throws -> LoadError {
        try #require(LoadError(openOf: "library-1", failedWith: error))
    }

    @Test("a failed open core offers the reset for offers Reset from cloud")
    func resetFromCloud() throws {
        let failure = try openFailure(.ResetFromCloud(error: damaged))
        #expect(failure.offersReset)
        #expect(failure.step == .libraryOpen)
        #expect(failure.libraryId == "library-1")
    }

    @Test(
        "a damaged library core fails plainly, as one with no storage, offers no reset"
    )
    func failed() throws {
        #expect(try !openFailure(.Failed(error: damaged)).offersReset)
    }

    @Test("a cancelled open shows nothing")
    func cancelled() {
        #expect(
            LoadError(
                openOf: "library-1",
                failedWith: BridgeOpenError.Failed(error: .Cancelled)
            ) == nil
        )
    }

    @Test("a failed removal that a retry cannot fix offers no Retry")
    func removalFailureWithoutRetry() throws {
        #expect(try !#require(DisplayError(damaged)).offersRetry)
        #expect(
            try #require(
                DisplayError(
                    BridgeError.Diagnostic(category: .network, detail: "x")
                )
            )
            .offersRetry
        )
    }
}
