import AppKit
import BaeKit
import SwiftUI
import Testing

/// The notice's buttons sit on the title's line without making the line
/// taller, as the boards draw them.
@MainActor
@Suite("Notice height")
struct MacNoticeHeightTests {
    private static func height(action: ErrorAction?) -> CGFloat {
        NSHostingView(
            rootView: MacNotice(
                title: "Audio left out",
                advice: "No track on this release matches these files.",
                error: nil,
                tone: .warning,
                action: action
            )
            .frame(width: 880)
        )
        .fittingSize.height
    }

    @Test("a notice with a button is as tall as one without")
    func buttonAddsNoHeight() {
        #expect(
            Self.height(action: ErrorAction("Pick another release…") {})
                == Self.height(action: nil)
        )
    }
}
