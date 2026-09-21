import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An inline field is as tall as a Text of its font, on every hosting.
///
/// The text field behind it clips to its own bounds, and a remount used to
/// hand it a height measured before its font applied, cutting the top off a
/// title. The field now lays itself out from a Text of the same font, so its
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
    /// between, draws the same pixels each time, and its field is exactly as
    /// tall as a Text of the title's font.
    @Test("the title field takes its height from its text, remount or not")
    func theTitleFieldTakesItsHeightFromItsText() async throws {
        let first = try await hostedHeader()
        let second = try await hostedHeader()
        #expect(
            first.pixels == second.pixels,
            "a remount draws the same header"
        )

        let measured = Measured()
        let (window, host) = SnapshotTestSupport.hostInWindow(
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
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        let textHeight = try #require(measured.textHeight)
        #expect(first.fieldHeight == textHeight)
        #expect(second.fieldHeight == textHeight)
    }

    /// One hosting of the header: its pixels, and the height of the title's
    /// text field as AppKit laid it out.
    private func hostedHeader() async throws -> (
        pixels: Data, fieldHeight: CGFloat
    ) {
        var seed = PreviewData.releaseEditSeed(trackCount: 2)
        seed.edit.albumTitle = Self.title
        let reset = seed.edit
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { _, _ in },
            reset: { _ in reset }
        )
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ReleaseMetadataHeader(
                values: session.form,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                cover: { EmptyView() },
                audioFacts: { EmptyView() }
            )
            .environment(Library.stub())
            .environment(UiStore())
            .preferredColorScheme(.light)
            .background(.white)
            .frame(width: Self.size.width, height: Self.size.height),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        let pixels = try await SnapshotTestSupport.capturePNG(
            host,
            size: Self.size
        )
        let titleField = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .first { $0.stringValue == Self.title }
        )
        return (pixels, titleField.frame.height)
    }
}
