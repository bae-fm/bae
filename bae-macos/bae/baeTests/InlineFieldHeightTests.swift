import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An inline field is as tall as a Text of its font, on every hosting.
///
/// The text field behind it clips to its own bounds, and a SwiftUI
/// `TextField` now and then handed it the height of another field in the
/// same tree, cutting the top off a title. The field now lays itself out
/// from a Text of the same font and edits in exactly that frame, so its
/// height is that Text's on the first hosting, on a remount, and everywhere
/// in between.
@MainActor
@Suite("Inline field height")
struct InlineFieldHeightTests {
    private static let size = NSSize(width: 900, height: 500)
    private static let title = "Original title"
    private static let titleFont = Font.system(size: 22, weight: .semibold)

    @MainActor
    private final class Measured {
        var textHeight: CGFloat?
    }

    /// The header's title field, hosted twice in one process with a remount
    /// between, is exactly as tall as a Text of the title's font each time.
    @Test("the title field takes its height from its text, remount or not")
    func theTitleFieldTakesItsHeightFromItsText() async throws {
        let first = try await hostedTitleFieldHeight()
        let second = try await hostedTitleFieldHeight()

        let measured = Measured()
        try await SnapshotTestSupport.withHostedWindow(
            Text(verbatim: Self.title)
                .font(Self.titleFont)
                .onGeometryChange(for: CGFloat.self) {
                    $0.size.height
                } action: {
                    measured.textHeight = $0
                }
                .frame(
                    width: Self.size.width,
                    height: Self.size.height,
                    alignment: .topLeading
                ),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let textHeight = try #require(measured.textHeight)
            #expect(first == textHeight)
            #expect(second == textHeight)
        }
    }

    /// Every field in the header is exactly as tall as AppKit measures a
    /// line of the field's own font, on every one of many hostings. The
    /// borrowed height came a few hostings in a thousand, so the check is
    /// made of many hostings rather than one.
    @Test("every field is as tall as its own font, hosting after hosting")
    func everyFieldIsAsTallAsItsOwnFont() async throws {
        // Title, album year, year and barcode, and a name and a catalog
        // number per label row.
        let labelRows = PreviewData.releaseEditDraft(trackCount: 2).edit
            .pressing.labels.count
        for _ in 0..<40 {
            try await withHostedHeader { _, host in
                try await SnapshotTestSupport.settle(host)
                let fields = SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .filter(\.isEditable)
                #expect(fields.count == 4 + 2 * labelRows)
                for field in fields {
                    #expect(
                        field.frame.height == field.intrinsicContentSize.height,
                        "\(field.font?.pointSize ?? 0)-point field is \(field.frame.height) tall"
                    )
                }
            }
        }
    }

    /// One hosting of the header: the height of the title's text field as
    /// AppKit laid it out.
    private func hostedTitleFieldHeight() async throws -> CGFloat {
        return try await withHostedHeader { _, host in
            try await SnapshotTestSupport.settle(host)
            let titleField = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first { $0.stringValue == Self.title }
            )
            return titleField.frame.height
        }
    }

    /// The release header over a two-track draft titled `title`, hosted.
    private func withHostedHeader<Value>(
        _ body: (NSWindow, NSView) async throws -> Value
    ) async throws -> Value {
        var draft = PreviewData.releaseEditDraft(trackCount: 2)
        draft.edit.albumTitle = Self.title
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            draft: draft
        )
        return try await SnapshotTestSupport.withHostedWindow(
            ReleaseMetadataHeader(
                values: session.draft.edit,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) },
                cover: { EmptyView() },
                audioFacts: { EmptyView() }
            )
            .environment(UiStore())
            .preferredColorScheme(.light)
            .background(.white)
            .frame(width: Self.size.width, height: Self.size.height),
            size: Self.size
        ) {
            try await body($0, $1)
        }
    }
}
