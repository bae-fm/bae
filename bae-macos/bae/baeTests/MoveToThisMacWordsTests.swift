import BaeKit
import Testing

@testable import bae

/// The Mac says Move to This Mac for the release action that detaches a
/// release's audio into a folder on this Mac, and for each state it passes
/// through.
@Suite("Move to This Mac words")
struct MoveToThisMacWordsTests {
    @Test("the release action reads Move to This Mac")
    func actionLabel() {
        #expect(
            BridgeReleaseStorageAction.detachAudio.label == "Move to This Mac…"
        )
    }

    @Test("the action while it runs reads Moving to This Mac")
    func runningLabel() {
        #expect(
            BridgeReleaseStorageAction.detachAudio.transferLabel
                == "Moving to This Mac…"
        )
    }

    @Test("the Activity panel line reads Moving to This Mac")
    func activityLine() {
        #expect(
            localizedCoreString("core.activity.making_local")
                == "Moving to This Mac"
        )
    }
}
