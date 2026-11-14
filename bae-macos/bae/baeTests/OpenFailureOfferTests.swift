import BaeKit
import Testing

@testable import bae

/// What a library that failed to open offers: Reset from cloud for damage in
/// this Mac's copy of a library that has storage, nothing to reset from
/// otherwise.
struct OpenFailureOfferTests {
    private func library(_ state: BridgeLibraryState) -> BridgeLibrary {
        BridgeLibrary(
            id: "library-1",
            name: "Library",
            path: "/libraries/library-1",
            state: state,
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        )
    }

    private func failure(_ category: BridgeErrorCategory) throws -> DisplayError
    {
        try #require(
            DisplayError(
                BridgeError.Diagnostic(category: category, detail: "x")
            )
        )
    }

    private let s3 = BridgeStorageLocation.s3(
        bucket: "music-library",
        region: "us-east-1",
        endpoint: nil,
        profile: nil
    )

    @Test("a damaged library with storage offers Reset from cloud")
    func damagedWithStorage() throws {
        #expect(
            library(.openable(storage: s3))
                .offersResetFromCloud(after: try failure(.databaseDamaged))
        )
    }

    @Test("a damaged library with no storage offers none, coven refuses it")
    func damagedWithoutStorage() throws {
        #expect(
            !library(.openable(storage: nil))
                .offersResetFromCloud(after: try failure(.databaseDamaged))
        )
    }

    @Test("a failure a new copy would meet again offers none")
    func notInTheCopy() throws {
        #expect(
            !library(.openable(storage: s3))
                .offersResetFromCloud(after: try failure(.network))
        )
    }

    @Test("a removal's failure that retrying can't pass offers no Retry")
    func removalFailureWithoutRetry() throws {
        #expect(try !failure(.databaseDamaged).offersRetry)
        #expect(try failure(.network).offersRetry)
    }
}
