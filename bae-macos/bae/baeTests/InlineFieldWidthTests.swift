import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An inline field that fits its value takes the value's width, and ends at
/// its column's edge when the value is wider than the column.
@MainActor
@Suite("Inline field width")
struct InlineFieldWidthTests {
    private static let size = NSSize(width: 400, height: 100)
    private static let columnWidth: CGFloat = 260

    private func hostedFieldWidth(of value: String) async throws -> CGFloat {
        try await SnapshotTestSupport.withHostedWindow(
            MacInlineField(
                placeholder: "—",
                value: value,
                fitsValue: true,
                onCommit: { _ in }
            )
            .frame(width: Self.columnWidth, alignment: .leading)
            .frame(
                width: Self.size.width,
                height: Self.size.height,
                alignment: .topLeading
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let field = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first { $0.stringValue == value }
            )
            // The text field's own frame reaches 2 points past the box on
            // each side; the view that hosts it is the box.
            return try #require(field.superview).frame.width
        }
    }

    /// The field pulls out by its leading padding, so a field that fills the
    /// column ends at the column's edge.
    @Test("a value wider than the column ends at the column's edge")
    func aLongValueEndsAtTheColumnsEdge() async throws {
        let width = try await hostedFieldWidth(
            of: "Harbor Records and the Northern Sound Company"
        )
        #expect(
            width
                == Self.columnWidth
                + FieldChrome.inlineHorizontalPadding
        )
    }

    @Test("a short value takes its own width")
    func aShortValueTakesItsOwnWidth() async throws {
        let short = try await hostedFieldWidth(of: "Harbor")
        let longer = try await hostedFieldWidth(of: "Harbor Records")
        #expect(short < longer)
        #expect(longer < Self.columnWidth)
    }
}
