import BaeKit
import Foundation
import Observation

/// The reviewed source revisions stay in core; this holds the person's order
/// and numbering choices and the preview that answers those choices.
@MainActor
@Observable
final class ImportCombinationReview {
    private let review: any CandidateCombinationReviewProtocol
    private(set) var keys: [String]
    private(set) var order: BridgeCombinationTrackOrder = .separateDiscs
    private(set) var preview: BridgeCombinationPreview
    var name = ""
    private(set) var error: DisplayError?
    private(set) var isSaving = false

    init(review: any CandidateCombinationReviewProtocol) throws {
        self.review = review
        let keys = review.candidateKeys()
        self.keys = keys
        preview = try review.preview(keys: keys, order: .separateDiscs)
    }

    var canSave: Bool {
        !isSaving
            && !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    func move(_ index: Int, to destination: Int) {
        guard keys.indices.contains(index), keys.indices.contains(destination)
        else { return }
        var reordered = keys
        reordered.swapAt(index, destination)
        update(keys: reordered, order: order)
    }

    func setOrder(_ order: BridgeCombinationTrackOrder) {
        update(keys: keys, order: order)
    }

    private func update(keys: [String], order: BridgeCombinationTrackOrder) {
        do {
            let preview = try review.preview(keys: keys, order: order)
            self.keys = keys
            self.order = order
            self.preview = preview
            error = nil
        }
        catch {
            self.error = DisplayError(error)
        }
    }

    func save() async -> String? {
        guard canSave else { return nil }
        isSaving = true
        defer { isSaving = false }
        do {
            return try await review.combine(
                keys: keys,
                order: order,
                name: name
            )
        }
        catch {
            self.error = DisplayError(error)
            return nil
        }
    }
}
