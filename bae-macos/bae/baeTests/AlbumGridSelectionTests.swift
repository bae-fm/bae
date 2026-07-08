import Testing

@testable import bae

@MainActor
@Suite("AlbumGridSelection")
struct AlbumGridSelectionTests {
    @Test("toggle adds, removes, and re-anchors on the clicked id")
    func toggleSetsUnsetsAndReanchors() {
        let selection = AlbumGridSelection()
        selection.toggle("a")
        #expect(selection.selectedIds == ["a"])
        #expect(selection.anchorId == "a")

        selection.toggle("b")
        #expect(selection.selectedIds == ["a", "b"])
        #expect(selection.anchorId == "b")

        selection.toggle("a")
        #expect(selection.selectedIds == ["b"])
        // A cmd-click re-anchors even when it removes the id.
        #expect(selection.anchorId == "a")
    }

    @Test("shift-range unions [anchor, target] in both directions")
    func shiftRangeUnionsBothDirections() {
        let ids = ["a", "b", "c", "d", "e"]
        let position: (String) -> Int? = { ids.firstIndex(of: $0) }
        let idAt: (Int) -> String? = {
            ids.indices.contains($0) ? ids[$0] : nil
        }

        let up = AlbumGridSelection()
        up.toggle("b")
        up.extendRange(to: "d", position: position, idAt: idAt)
        #expect(up.selectedIds == ["b", "c", "d"])
        // The anchor is unchanged by a range extend.
        #expect(up.anchorId == "b")

        let down = AlbumGridSelection()
        down.toggle("d")
        down.extendRange(to: "a", position: position, idAt: idAt)
        #expect(down.selectedIds == ["a", "b", "c", "d"])
        #expect(down.anchorId == "d")
    }

    @Test("shift-range skips ids in the span that aren't loaded")
    func shiftRangeSkipsUnloadedGaps() {
        // Index 2 is within the span but not loaded (idAt returns nil).
        let positions = ["a": 0, "b": 1, "d": 3]
        let loaded = [0: "a", 1: "b", 3: "d"]
        let selection = AlbumGridSelection()
        selection.toggle("a")
        selection.extendRange(
            to: "d",
            position: { positions[$0] },
            idAt: { loaded[$0] }
        )
        #expect(selection.selectedIds == ["a", "b", "d"])
    }

    @Test("shift-range degrades to a toggle when the anchor no longer resolves")
    func missingAnchorDegradesToToggle() {
        let selection = AlbumGridSelection()
        selection.toggle("a")
        selection.extendRange(
            to: "c",
            position: { _ in nil },
            idAt: { _ in nil }
        )
        #expect(selection.selectedIds == ["a", "c"])
        #expect(selection.anchorId == "c")
    }

    @Test("selectAll selects every id and anchors the last; clear empties")
    func selectAllAndClear() {
        let selection = AlbumGridSelection()
        selection.selectAll(["a", "b", "c"])
        #expect(selection.selectedIds == ["a", "b", "c"])
        #expect(selection.anchorId == "c")

        selection.clear()
        #expect(selection.selectedIds.isEmpty)
        #expect(selection.anchorId == nil)
    }

    @Test(
        "orderedTargets returns visible order for a member of a multi-selection"
    )
    func orderedTargetsVisibleOrder() {
        let positions = ["a": 0, "b": 1, "c": 2]
        let selection = AlbumGridSelection()
        selection.toggle("c")
        selection.toggle("a")
        #expect(
            selection.orderedTargets(for: "a", position: { positions[$0] })
                == ["a", "c"]
        )
    }

    @Test("orderedTargets drops ids that don't resolve to a position")
    func orderedTargetsDropsUnresolvable() {
        let selection = AlbumGridSelection()
        selection.toggle("a")
        selection.toggle("b")
        #expect(
            selection.orderedTargets(
                for: "a",
                position: { $0 == "a" ? 0 : nil }
            )
                == ["a"]
        )
    }

    @Test("orderedTargets returns just the clicked id for a non-member click")
    func orderedTargetsNonMember() {
        let positions = ["a": 0, "b": 1, "c": 2]
        let selection = AlbumGridSelection()
        selection.toggle("a")
        selection.toggle("c")
        #expect(
            selection.orderedTargets(for: "b", position: { positions[$0] })
                == ["b"]
        )
    }

    @Test("orderedTargets returns just the clicked id for a single selection")
    func orderedTargetsSingleSelection() {
        let selection = AlbumGridSelection()
        selection.toggle("a")
        #expect(
            selection.orderedTargets(for: "a", position: { _ in 0 }) == ["a"]
        )
    }

    @Test("remove drops only the missing ids and clears a removed anchor")
    func pruneRemovesOnlyMissing() {
        let selection = AlbumGridSelection()
        selection.selectAll(["a", "b", "c"])
        selection.remove(["b"])
        #expect(selection.selectedIds == ["a", "c"])
        // The anchor (c) survives when it isn't among the removed ids.
        #expect(selection.anchorId == "c")

        selection.remove(["c"])
        #expect(selection.selectedIds == ["a"])
        #expect(selection.anchorId == nil)
    }
}
