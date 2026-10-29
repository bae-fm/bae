import Testing

@testable import bae

@MainActor
@Suite("AlbumGridSelection")
struct AlbumGridSelectionTests {
    @Test("a range ends at the clicked collaboration and selects albums once")
    func rangeUsesClickedAppearance() {
        let ids = ["shared", "first", "second", "shared", "last"]
        let selection = AlbumGridSelection()
        selection.setSelected("second", selected: true, sectionId: "artist-b")
        selection.extendRange(
            to: "shared",
            targetPosition: 3,
            position: { ids.firstIndex(of: $0) },
            idAt: { ids[$0] }
        )
        #expect(selection.selectedIds == ["second", "shared"])
        #expect(selection.anchor?.sectionId == "artist-b")
        selection.extendRange(
            to: "shared",
            targetPosition: 0,
            position: { ids.firstIndex(of: $0) },
            idAt: { ids[$0] }
        )
        #expect(
            Set(selection.targets(for: "shared"))
                == ["shared", "first", "second"]
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

    @Test("shift-range unions [anchor, target] in both directions")
    func shiftRangeUnionsBothDirections() {
        let ids = ["a", "b", "c", "d", "e"]
        let position: (String) -> Int? = { ids.firstIndex(of: $0) }
        let idAt: (Int) -> String? = {
            ids.indices.contains($0) ? ids[$0] : nil
        }

        let up = AlbumGridSelection()
        up.setSelected("b", selected: true)
        up.extendRange(to: "d", position: position, idAt: idAt)
        #expect(up.selectedIds == ["b", "c", "d"])
        // The anchor is unchanged by a range extend.
        #expect(up.anchor?.albumId == "b")

        let down = AlbumGridSelection()
        down.setSelected("d", selected: true)
        down.extendRange(to: "a", position: position, idAt: idAt)
        #expect(down.selectedIds == ["a", "b", "c", "d"])
        #expect(down.anchor?.albumId == "d")
    }

    @Test("shift-range skips ids in the span that aren't loaded")
    func shiftRangeSkipsUnloadedGaps() {
        // Index 2 is within the span but not loaded (idAt returns nil).
        let positions = ["a": 0, "b": 1, "d": 3]
        let loaded = [0: "a", 1: "b", 3: "d"]
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.extendRange(
            to: "d",
            position: { positions[$0] },
            idAt: { loaded[$0] }
        )
        #expect(selection.selectedIds == ["a", "b", "d"])
    }

    @Test(
        "shift-range selects the clicked album when the anchor no longer resolves"
    )
    func missingAnchorSelectsTarget() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true)
        selection.extendRange(
            to: "c",
            position: { _ in nil },
            idAt: { _ in nil }
        )
        #expect(selection.selectedIds == ["a", "c"])
        #expect(selection.anchor?.albumId == "c")
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
