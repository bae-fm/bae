import Testing

@testable import bae

@Suite("Picked tracks")
struct PickedTracksTests {
    @Test("setting a row's state picks or unpicks it")
    func setsMembership() {
        let picked = PickedTracks()
        picked.set("b", picked: true)
        picked.set("d", picked: true)
        #expect(picked.ids == ["b", "d"])
        picked.set("b", picked: false)
        #expect(picked.ids == ["d"])
    }

    @Test("shift picks the range core read, keeping the anchor")
    func extendsOverTheRange() {
        let picked = PickedTracks()
        picked.set("b", picked: true)
        picked.extend(over: ["b", "c", "d"])
        #expect(picked.ids == ["b", "c", "d"])
        picked.extend(over: ["a", "b"])
        #expect(picked.ids == ["a", "b", "c", "d"])
        #expect(picked.anchorId == "b")
    }

    @Test("a plain click clears the picks and the anchor")
    func clears() {
        let picked = PickedTracks()
        picked.set("a", picked: true)
        picked.clear()
        #expect(picked.ids.isEmpty)
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
