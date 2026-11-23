import AppKit
import SwiftUI
import Testing

@testable import bae

@Suite("Flow layout")
struct FlowLayoutTests {
    private static let width: CGFloat = 200

    @MainActor
    private func height<Content: View>(
        @ViewBuilder _ content: () -> Content
    ) -> CGFloat {
        NSHostingView(
            rootView: FlowLayout { content() }
                .frame(width: Self.width, alignment: .leading)
        )
        .fittingSize.height
    }

    @MainActor
    @Test("an item wider than the row wraps inside it and makes the row taller")
    func wideItemWrapsInsideTheRow() {
        let one = height { Text(verbatim: "one line") }
        let wrapped = height {
            Text(
                verbatim: Array(repeating: "KBOX3555B", count: 12)
                    .joined(separator: " ")
            )
        }
        #expect(wrapped > one * 2)
    }
}
