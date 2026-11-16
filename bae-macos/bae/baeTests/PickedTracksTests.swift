import Testing

@testable import bae

@MainActor
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

    @Test("shift-click picks the range that core read and keeps the anchor")
    func extendsOverTheRange() async {
        let picked = PickedTracks()
        let failures = Failures()
        picked.set("b", picked: true)
        picked.extend(
            to: "d",
            readRange: { anchorId, targetId in
                #expect(anchorId == "b")
                #expect(targetId == "d")
                return ["b", "c", "d"]
            },
            onFailure: failures.record
        )
        await picked.rangeRead?.value
        #expect(picked.ids == ["b", "c", "d"])
        #expect(picked.anchorId == "b")
        #expect(failures.errors.isEmpty)
    }

    @Test(
        "shift-click with no anchor picks the clicked row alone and anchors it"
    )
    func extendsWithNoAnchor() {
        let picked = PickedTracks()
        let failures = Failures()
        picked.extend(
            to: "c",
            readRange: { _, _ in
                Issue.record("a shift-click with no anchor reads no range")
                return []
            },
            onFailure: failures.record
        )
        #expect(picked.rangeRead == nil)
        #expect(picked.ids == ["c"])
        #expect(picked.anchorId == "c")
    }

    @Test("a failed range read picks nothing and passes the error on")
    func failedRangeReadPicksNothing() async {
        let picked = PickedTracks()
        let failures = Failures()
        picked.set("b", picked: true)
        picked.extend(
            to: "d",
            readRange: { _, _ in throw RangeReadFailed() },
            onFailure: failures.record
        )
        await picked.rangeRead?.value
        #expect(picked.ids == ["b"])
        #expect(failures.errors.count == 1)
    }

    @Test("clearing the picks cancels a range read under way")
    func clearCancelsTheRangeRead() async {
        let picked = PickedTracks()
        let failures = Failures()
        picked.set("b", picked: true)
        picked.extend(
            to: "d",
            readRange: { _, _ in
                try await Task.sleep(for: .seconds(60))
                return ["b", "c", "d"]
            },
            onFailure: failures.record
        )
        let read = picked.rangeRead
        picked.clear()
        await read?.value
        #expect(picked.ids.isEmpty)
        #expect(failures.errors.isEmpty)
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

/// The errors a range read passed to `onFailure`.
@MainActor
private final class Failures {
    private(set) var errors: [any Error] = []

    func record(_ error: any Error) {
        errors.append(error)
    }
}

private struct RangeReadFailed: Error {}
