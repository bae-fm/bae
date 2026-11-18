import Testing

@testable import bae

@MainActor
@Suite("AlbumGridSelection")
struct AlbumGridSelectionTests {
    @Test("a shift-click selects the albums core reads between the two cards")
    func rangeIsCoresAnswer() async throws {
        let reads = RangeReads(answer: ["second", "shared", "last"])
        let selection = AlbumGridSelection()
        selection.setSelected("second", selected: true, sectionId: "artist-b")
        selection.extendRange(
            to: "shared",
            sectionId: "artist-c",
            readRange: { try await reads.read($0, $1) },
            onFailure: { Issue.record($0) }
        )
        try await Wait.until { selection.selectedIds.count == 3 }

        #expect(selection.selectedIds == ["second", "shared", "last"])
        #expect(
            reads.asked
                == [
                    .init(
                        from: .init(albumId: "second", sectionId: "artist-b"),
                        to: .init(albumId: "shared", sectionId: "artist-c")
                    )
                ]
        )
        // The anchor is unchanged by a range extend.
        #expect(
            selection.anchor == .init(albumId: "second", sectionId: "artist-b")
        )
    }

    @Test("selection commands set membership and anchor at the clicked album")
    func membershipCommandsAreIdempotent() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        #expect(selection.selectedIds == ["a"])
        #expect(selection.anchor?.albumId == "a")

        selection.setSelected("b", selected: true)
        #expect(selection.selectedIds == ["a", "b"])
        #expect(selection.anchor?.albumId == "b")

        selection.setSelected("a", selected: false)
        selection.setSelected("a", selected: false)
        #expect(selection.selectedIds == ["b"])
        // A cmd-click re-anchors even when it removes the id.
        #expect(selection.anchor?.albumId == "a")
    }

    @Test("a shift-click with no anchor selects and anchors the clicked album")
    func noAnchorSelectsTarget() {
        let selection = AlbumGridSelection()
        selection.extendRange(
            to: "c",
            sectionId: "artist-a",
            readRange: { _, _ in
                Issue.record("a shift-click with no anchor reads no range")
                return []
            },
            onFailure: { Issue.record($0) }
        )
        #expect(selection.selectedIds == ["c"])
        #expect(selection.anchor == .init(albumId: "c", sectionId: "artist-a"))
    }

    @Test("a click while core reads the range keeps the range out")
    func clickCancelsTheRangeRead() async throws {
        let reads = RangeReads(answer: ["a", "b", "c"], held: true)
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.extendRange(
            to: "c",
            readRange: { try await reads.read($0, $1) },
            onFailure: { Issue.record($0) }
        )
        try await Wait.until { !reads.asked.isEmpty }
        selection.setSelected("d", selected: true)
        reads.release()
        try await Wait.until { reads.answered }

        #expect(selection.selectedIds == ["a", "d"])
    }

    @Test("a failed range read selects nothing and reports the failure")
    func failedRangeReadReports() async throws {
        let reads = RangeReads(answer: nil)
        let failure = FailureRecord()
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.extendRange(
            to: "c",
            readRange: { try await reads.read($0, $1) },
            onFailure: { failure.error = $0 }
        )
        try await Wait.until { failure.error != nil }

        #expect(selection.selectedIds == ["a"])
    }

    @Test("clear empties the selection and its anchor")
    func clearEmpties() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.setSelected("b", selected: true)
        #expect(selection.anchor?.albumId == "b")

        selection.clear()
        #expect(selection.selectedIds.isEmpty)
        #expect(selection.anchor?.albumId == nil)
    }

    @Test("targets are the whole selection for a member of a multi-selection")
    func targetsTheSelection() {
        let selection = AlbumGridSelection()
        selection.setSelected("c", selected: true)
        selection.setSelected("a", selected: true)
        // In no order: core orders them as the grid lists them.
        #expect(Set(selection.targets(for: "a")) == ["a", "c"])
    }

    @Test("targets are just the clicked id for a non-member click")
    func targetsNonMember() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.setSelected("c", selected: true)
        #expect(selection.targets(for: "b") == ["b"])
    }

    @Test("targets are just the clicked id for a single selection")
    func targetsSingleSelection() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        #expect(selection.targets(for: "a") == ["a"])
    }

    @Test("remove drops only the missing ids and clears a removed anchor")
    func pruneRemovesOnlyMissing() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.setSelected("b", selected: true)
        selection.setSelected("c", selected: true)
        selection.remove(["b"])
        #expect(selection.selectedIds == ["a", "c"])
        // The anchor (c) survives when it isn't among the removed ids.
        #expect(selection.anchor?.albumId == "c")

        selection.remove(["c"])
        #expect(selection.selectedIds == ["a"])
        #expect(selection.anchor?.albumId == nil)
    }
}

/// Stands in for core's read of the albums between two cards: records each
/// pair of cards it is asked for and answers `answer`, or fails with none,
/// once released when `held`.
@MainActor
private final class RangeReads {
    struct Ask: Equatable {
        let from: AlbumGridSelection.Anchor
        let to: AlbumGridSelection.Anchor
    }

    private struct NoAnswer: Error {}

    private let answer: [String]?
    private let held: Bool
    private var waiting: CheckedContinuation<Void, Never>?
    private(set) var asked: [Ask] = []
    private(set) var answered = false

    init(answer: [String]?, held: Bool = false) {
        self.answer = answer
        self.held = held
    }

    func read(
        _ from: AlbumGridSelection.Anchor,
        _ to: AlbumGridSelection.Anchor
    ) async throws -> [String] {
        asked.append(Ask(from: from, to: to))
        if held {
            await withCheckedContinuation { waiting = $0 }
        }
        answered = true
        guard let answer else { throw NoAnswer() }
        return answer
    }

    /// Answer the read waiting on the release.
    func release() {
        waiting?.resume()
        waiting = nil
    }
}

@MainActor
private final class FailureRecord {
    var error: (any Error)?
}
