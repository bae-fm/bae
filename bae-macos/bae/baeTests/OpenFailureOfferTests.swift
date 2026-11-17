import BaeKit
import Testing

@testable import bae

/// A library that failed to open offers Reset from cloud when core says a
/// reset from the cloud can load it again, and offers nothing otherwise.
struct OpenFailureOfferTests {
    private let damaged = BridgeError.Diagnostic(
        category: .databaseDamaged,
        detail: "x"
    )

    private func openFailure(_ error: BridgeOpenError) throws -> LoadError {
        try #require(LoadError(openOf: "library-1", failedWith: error))
    }

    @Test("an open that core says a reset can fix offers Reset from cloud")
    func resetFromCloud() throws {
        let failure = try openFailure(.ResetFromCloud(error: damaged))
        #expect(failure.step == .libraryOpen(offersReset: true))
        #expect(failure.libraryId == "library-1")
    }

    @Test(
        "a damaged library that core fails without offering a reset, such as one with no storage, offers no reset"
    )
    func failed() throws {
        #expect(
            try openFailure(.Failed(error: damaged)).step
                == .libraryOpen(offersReset: false)
        )
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
