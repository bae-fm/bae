import Testing

@testable import bae

@MainActor
@Suite("AlbumGridSelection")
struct AlbumGridSelectionTests {
    @Test("a range ends at the clicked collaboration and selects albums once")
    func rangeUsesClickedAppearance() {
        let ids = ["shared", "first", "second", "shared", "last"]
        let selection = AlbumGridSelection()
        selection.setSelected(
            "second",
            selected: true,
            position: 2,
            sectionId: "artist-b"
        )
        selection.extendRange(to: "shared", position: 3, idAt: { ids[$0] })
        #expect(selection.selectedIds == ["second", "shared"])
        #expect(selection.anchor?.sectionId == "artist-b")
        selection.extendRange(to: "shared", position: 0, idAt: { ids[$0] })
        #expect(
            Set(selection.targets(for: "shared"))
                == ["shared", "first", "second"]
        )
    }

    @Test("selection commands set membership and anchor at the clicked album")
    func membershipCommandsAreIdempotent() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        #expect(selection.selectedIds == ["a"])
        #expect(selection.anchor?.albumId == "a")

        selection.setSelected("b", selected: true, position: 1)
        #expect(selection.selectedIds == ["a", "b"])
        #expect(selection.anchor?.albumId == "b")

        selection.setSelected("a", selected: false, position: 0)
        selection.setSelected("a", selected: false, position: 0)
        #expect(selection.selectedIds == ["b"])
        // A cmd-click re-anchors even when it removes the id.
        #expect(selection.anchor?.albumId == "a")
    }

    @Test("shift-range unions [anchor, target] in both directions")
    func shiftRangeUnionsBothDirections() {
        let ids = ["a", "b", "c", "d", "e"]
        let idAt: (Int) -> String? = {
            ids.indices.contains($0) ? ids[$0] : nil
        }

        let up = AlbumGridSelection()
        up.setSelected("b", selected: true, position: 1)
        up.extendRange(to: "d", position: 3, idAt: idAt)
        #expect(up.selectedIds == ["b", "c", "d"])
        // The anchor is unchanged by a range extend.
        #expect(up.anchor?.albumId == "b")

        let down = AlbumGridSelection()
        down.setSelected("d", selected: true, position: 3)
        down.extendRange(to: "a", position: 0, idAt: idAt)
        #expect(down.selectedIds == ["a", "b", "c", "d"])
        #expect(down.anchor?.albumId == "d")
    }

    @Test("shift-range skips ids in the span that aren't loaded")
    func shiftRangeSkipsUnloadedGaps() {
        // Index 2 is within the span but not loaded (idAt returns nil).
        let loaded = [0: "a", 1: "b", 3: "d"]
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        selection.extendRange(to: "d", position: 3, idAt: { loaded[$0] })
        #expect(selection.selectedIds == ["a", "b", "d"])
    }

    @Test(
        "shift-range selects the clicked album once the list dropped the anchor"
    )
    func droppedAnchorSelectsTarget() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        selection.dropAnchor()
        #expect(selection.selectedIds == ["a"])
        selection.extendRange(to: "c", position: 2, idAt: { _ in nil })
        #expect(selection.selectedIds == ["a", "c"])
        #expect(selection.anchor?.albumId == "c")
        #expect(selection.anchor?.position == 2)
    }

    @Test("clear empties the selection and its anchor")
    func clearEmpties() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        selection.setSelected("b", selected: true, position: 1)
        #expect(selection.anchor?.albumId == "b")

        selection.clear()
        #expect(selection.selectedIds.isEmpty)
        #expect(selection.anchor?.albumId == nil)
    }

    @Test("targets are the whole selection for a member of a multi-selection")
    func targetsTheSelection() {
        let selection = AlbumGridSelection()
        selection.setSelected("c", selected: true, position: 2)
        selection.setSelected("a", selected: true, position: 0)
        // In no order: core orders them as the grid lists them.
        #expect(Set(selection.targets(for: "a")) == ["a", "c"])
    }

    @Test("targets are just the clicked id for a non-member click")
    func targetsNonMember() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        selection.setSelected("c", selected: true, position: 2)
        #expect(selection.targets(for: "b") == ["b"])
    }

    @Test("targets are just the clicked id for a single selection")
    func targetsSingleSelection() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        #expect(selection.targets(for: "a") == ["a"])
    }

    @Test("remove drops only the missing ids and clears a removed anchor")
    func pruneRemovesOnlyMissing() {
        let selection = AlbumGridSelection()
        selection.setSelected("a", selected: true, position: 0)
        selection.setSelected("b", selected: true, position: 1)
        selection.setSelected("c", selected: true, position: 2)
        selection.remove(["b"])
        #expect(selection.selectedIds == ["a", "c"])
        // The anchor (c) survives when it isn't among the removed ids.
        #expect(selection.anchor?.albumId == "c")

        selection.remove(["c"])
        #expect(selection.selectedIds == ["a"])
        #expect(selection.anchor?.albumId == nil)
    }
}
