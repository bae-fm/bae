import BaeKit
import Testing

@testable import bae

@Suite("Why the Subsonic server isn't running")
struct SubsonicNotRunningWordsTests {
    private let names = NetworkNames(byInterface: ["en0": "Wi-Fi"])

    @Test("a network is named as System Settings names it")
    func networkGoneUsesTheSystemName() {
        #expect(
            BridgeSubsonicNotRunning.networkGone(interface: "en0")
                .line(names: names) == "Wi-Fi is not connected"
        )
    }

    @Test("an interface the system has no name for is named by its BSD name")
    func unnamedInterfaceKeepsItsBSDName() {
        #expect(names.name(of: "utun9") == "utun9")
    }

    @Test("a failure reads as what the system said")
    func failureIsTheDetail() {
        #expect(
            BridgeSubsonicNotRunning.failed(detail: "Permission denied")
                .line(names: names) == "Permission denied"
        )
    }
}
