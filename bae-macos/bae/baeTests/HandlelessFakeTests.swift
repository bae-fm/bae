import BaeKit
import Foundation
import Testing

/// A fake built with `init(noHandle:)` has no core object behind it. One that
/// reaches core through a method it doesn't override stops with a Swift
/// precondition naming its class and that method, rather than faulting in
/// Rust on handle 0 and leaving a crash report that reads as an app crash.
@Suite("Handle-less fakes")
struct HandlelessFakeTests {
    @Test(
        "a handle-less fake that reaches core stops, naming itself and the method"
    )
    func aHandlelessFakeReachingCoreStops() async throws {
        let result = await #expect(
            processExitsWith: .signal(SIGTRAP),
            observing: [\.standardErrorContent]
        ) {
            _ = try await AppHandle(noHandle: AppHandle.NoHandle()).people()
        }
        let exit = try #require(result)
        let message = try #require(
            String(bytes: exit.standardErrorContent, encoding: .utf8)
        )
        #expect(
            message.contains(
                "a handle-less AppHandle reached core from people()"
            )
        )
    }
}
