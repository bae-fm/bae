import BaeKit
import Testing

/// A screen's reading of a stream value that changes now and then, such as
/// where a join stands.
@MainActor
@Suite("FollowedValue")
struct FollowedValueTests {
    /// The value starts as the stream holds it, follows each change while
    /// the key holds one, and goes as the key does.
    @Test("follows the key's value as it changes and goes")
    func followsTheValue() async throws {
        let writer = LatestStream<BridgeBootstrapState>.written()
        let connecting = BridgeBootstrapState(
            status: .connecting,
            cancellable: true
        )
        writer.set(connecting)
        let followed = FollowedValue(writer.stream.only)
        #expect(followed.value == connecting)

        let loading = BridgeBootstrapState(
            status: .loading(
                progress: BridgeLoadProgress(step: .reading, done: 2, total: 8)
            ),
            cancellable: false
        )
        writer.set(loading)
        try await Task.sleep(for: .milliseconds(300))
        #expect(followed.value == loading)

        writer.remove()
        try await Task.sleep(for: .milliseconds(300))
        #expect(followed.value == nil)
        followed.stop()
    }
}
