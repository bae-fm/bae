import BaeKit
import Testing

@testable import bae

/// A fact with several values lists every one in one line.
@Suite("Stated values")
struct StatedValueTests {
    private var separator: String {
        localizedCoreString("core.pressing.alternatives_separator")
    }

    @Test("an album's titles are every title, in one line")
    func titlesListEveryTitle() {
        #expect(
            PressingText.alternatives([
                "Low Tide Hours", "Low Tide Hours (Remastered)",
            ])
                == "Low Tide Hours" + separator + "Low Tide Hours (Remastered)"
        )
    }
}
