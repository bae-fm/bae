import BaeKit
import Testing

@testable import bae

/// A reset that failed offers Sign in again when core says the provider
/// refused the sign-in the reset started from, and offers it for nothing
/// else.
struct ResetFailureOfferTests {
    @Test("a reset whose kept sign-in Dropbox refused offers Sign in again")
    func signedOut() throws {
        let failure = try #require(
            LoadError(
                resetFailedWith: BridgeError.Diagnostic(
                    category: .signedOut(provider: .dropbox),
                    detail: "x"
                )
            )
        )
        #expect(failure.step == .deviceReset(offersSignIn: true))
    }

    @Test("a reset that failed another way offers no sign-in")
    func network() throws {
        let failure = try #require(
            LoadError(
                resetFailedWith: BridgeError.Diagnostic(
                    category: .network,
                    detail: "x"
                )
            )
        )
        #expect(failure.step == .deviceReset(offersSignIn: false))
    }
}
