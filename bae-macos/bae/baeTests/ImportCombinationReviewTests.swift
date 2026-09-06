import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ImportCombinationReviewTests {
    @Test
    func blankNameCannotBeSubmitted() async throws {
        let review = try ImportCombinationReview(
            review: ImportCombinationPreviewReply()
        )
        #expect(!review.canSave)
        review.name = " \n "
        #expect(await review.save() == nil)
        #expect(review.error == nil)
        review.name = "Collected Volumes"
        #expect(review.canSave)
    }

    @Test
    func rejectedPreviewPreservesTheReviewedOrder() throws {
        let review = try ImportCombinationReview(
            review: ImportCombinationPreviewReply()
        )
        review.move(0, to: 1)
        #expect(review.keys == ImportCombinationPreviewReply.keys)
        #expect(review.preview.parts.map(\.candidateKey) == review.keys)
        #expect(review.error != nil)
        review.setOrder(.continuous)
        #expect(review.order == .separateDiscs)
        #expect(review.preview.tracks.map(\.side) == [1, 1, 2, 2])
    }

    @Test
    func failedSubmissionKeepsTheDraftAvailable() async throws {
        let review = try ImportCombinationReview(
            review: ImportCombinationPreviewReply()
        )
        review.name = "Collected Volumes"
        #expect(await review.save() == nil)
        #expect(review.error != nil)
        #expect(review.name == "Collected Volumes")
        #expect(!review.isSaving)
        #expect(review.canSave)
    }

    @Test
    func reviewRendersTheProductionControls() async throws {
        let size = NSSize(width: 936, height: 696)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCombinationPreviewScene()
                .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        #expect(!png.isEmpty)
    }
}
