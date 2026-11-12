import BaeKit
import Testing

/// What a notice's Details opens: the line it shows, or the line it leaves out,
/// over the causes, and the whole detail to copy.
@Suite("ErrorDetails")
struct ErrorDetailsTests {
    @Test("a line with no detail has no Details")
    func lineWithoutDetail() {
        #expect(ErrorDetails(error: DisplayError(line: "Offline")) == nil)
    }

    @Test("a line with a long detail opens with the line over the excerpt")
    func lineWithDetail() throws {
        let detail = String(repeating: "x", count: 500)
        let error = DisplayError(line: "Something failed", detail: detail)

        let details = try #require(ErrorDetails(error: error))

        #expect(details.title == "Something failed")
        #expect(details.message == nil)
        #expect(details.causes == error.detailExcerpt)
        #expect(details.causes.count < detail.count)
        #expect(details.copyText == detail)
    }

    @Test("a titled notice leads its causes and copy with the line")
    func titledLeadsWithLine() {
        let error = DisplayError(line: "Missing", detail: "OAuthConfig: no id")

        let details = ErrorDetails(
            title: "Sign-in settings",
            advice: "Reinstall bae.",
            error: error
        )

        #expect(details.title == "Sign-in settings")
        #expect(details.message == "Reinstall bae.")
        #expect(details.causes == "Missing\nOAuthConfig: no id")
        #expect(details.copyText == "Missing\nOAuthConfig: no id")
    }

    @Test("a titled notice with no detail has the line alone")
    func titledWithoutDetail() {
        let details = ErrorDetails(
            title: "Could not remove",
            advice: nil,
            error: DisplayError(line: "In use")
        )

        #expect(details.message == nil)
        #expect(details.causes == "In use")
        #expect(details.copyText == "In use")
    }
}
