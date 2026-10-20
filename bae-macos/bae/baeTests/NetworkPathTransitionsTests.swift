import BaeKit
import Testing

/// The network path reports to sync only when it becomes usable: the first
/// satisfied path, and a satisfied path over a different interface — never
/// an update about a path already reported.
@Suite("NetworkPathTransitions")
struct NetworkPathTransitionsTests {
    /// Whether each of `paths`, in turn, is a change worth reporting.
    private func reports(_ paths: [NetworkPathTransitions.Path]) -> [Bool] {
        var transitions = NetworkPathTransitions()
        return paths.map { transitions.update($0) }
    }

    @Test("the first satisfied path reports, and repeats of it do not")
    func firstSatisfiedPathReportsOnce() {
        #expect(
            reports([
                .satisfied(interface: "en0"),
                .satisfied(interface: "en0"),
            ]) == [true, false]
        )
    }

    @Test("a path satisfied again after losing the network reports")
    func satisfiedAgainAfterUnsatisfiedReports() {
        #expect(
            reports([
                .satisfied(interface: "en0"),
                .unsatisfied,
                .unsatisfied,
                .satisfied(interface: "en0"),
            ]) == [true, false, false, true]
        )
    }

    @Test("moving to another interface reports")
    func movingToAnotherInterfaceReports() {
        #expect(
            reports([
                .satisfied(interface: "en0"),
                .satisfied(interface: "pdp_ip0"),
            ]) == [true, true]
        )
    }

    @Test("an unsatisfied path never reports")
    func unsatisfiedNeverReports() {
        #expect(reports([.unsatisfied]) == [false])
    }
}
