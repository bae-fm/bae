import Foundation
import Testing

@testable import bae

/// The app hands core its language at launch and again when the system's
/// changes; a failure is shown, not dropped.
@MainActor
@Suite("Sort language")
struct SortLanguageTests {
    /// What the app tells core, in order, read as it arrives.
    private final class Told: @unchecked Sendable {
        let stream: AsyncStream<String>
        private let continuation: AsyncStream<String>.Continuation

        init() {
            let made = AsyncStream.makeStream(of: String.self)
            stream = made.stream
            continuation = made.continuation
        }

        func tell(_ tag: String) {
            continuation.yield(tag)
        }
    }

    @Test("the first preferred language is told at launch and after a change")
    func tellsCoreAtLaunchAndOnChange() async {
        let told = Told()
        var preferred = ["sv-SE", "en"]
        let sortLanguage = SortLanguage(
            set: { told.tell($0) },
            languages: { preferred },
            showError: { _ in }
        )
        sortLanguage.start()
        var tags = told.stream.makeAsyncIterator()
        #expect(await tags.next() == "sv-SE")

        preferred = ["ru-RU"]
        NotificationCenter.default.post(
            name: NSLocale.currentLocaleDidChangeNotification,
            object: nil
        )
        #expect(await tags.next() == "ru-RU")
    }

    @Test("no preferred language is the root order")
    func rootOrderWhenNoneIsPreferred() {
        let sortLanguage = SortLanguage(
            set: { _ in },
            languages: { [] },
            showError: { _ in }
        )
        #expect(sortLanguage.tag == "und")
    }

    @Test("a refusal from core is shown")
    func showsARefusal() async {
        struct Refused: Error {}
        let shown = Told()
        let sortLanguage = SortLanguage(
            set: { _ in throw Refused() },
            languages: { ["sv"] },
            showError: { shown.tell("\($0)") }
        )
        sortLanguage.start()
        var errors = shown.stream.makeAsyncIterator()
        #expect(await errors.next() != nil)
    }
}
