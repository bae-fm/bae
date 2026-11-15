import Testing

@testable import bae

@Suite("Picked tracks")
struct PickedTracksTests {
    private let rows = ["a", "b", "c", "d", "e"]

    @Test("a row is picked and unpicked by setting its state")
    func setsMembership() {
        let picked = PickedTracks()
        picked.set("b", picked: true)
        picked.set("d", picked: true)
        #expect(picked.ids == ["b", "d"])
        picked.set("b", picked: false)
        #expect(picked.ids == ["d"])
    }

    @Test("shift picks the rows from the anchor to the target, either way")
    func extendsFromTheAnchor() {
        let picked = PickedTracks()
        picked.set("b", picked: true)
        picked.extend(to: "d", in: rows)
        #expect(picked.ids == ["b", "c", "d"])
        picked.extend(to: "a", in: rows)
        #expect(picked.ids == ["a", "b", "c", "d"])
        #expect(picked.anchorId == "b")
    }

    @Test("shift without an anchor on the page picks the target alone")
    func extendsWithoutAnAnchor() {
        let picked = PickedTracks()
        picked.extend(to: "c", in: rows)
        #expect(picked.ids == ["c"])
        #expect(picked.anchorId == "c")
    }

    @Test("a plain click clears the picks and the anchor")
    func clears() {
        let picked = PickedTracks()
        picked.set("a", picked: true)
        picked.clear()
        #expect(picked.ids.isEmpty)
        #expect(picked.anchorId == nil)
    }

    @Test("rows the page no longer lists leave the picks")
    func keepsOnlyListedRows() {
        let picked = PickedTracks()
        picked.set("a", picked: true)
        picked.set("b", picked: true)
        picked.keep(["a", "c"])
        #expect(picked.ids == ["a"])
        #expect(picked.anchorId == nil)
    }

    @Test("a menu on a picked row acts on every pick, on any other row alone")
    func menuTargets() {
        let picked = PickedTracks()
        picked.set("a", picked: true)
        #expect(picked.targets(for: "a") == ["a"])
        picked.set("c", picked: true)
        #expect(Set(picked.targets(for: "c")) == ["a", "c"])
        #expect(picked.targets(for: "b") == ["b"])
        #expect(picked.ids == ["a", "c"])
    }
}
