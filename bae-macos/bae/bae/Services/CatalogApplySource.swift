import BaeKit

/// One apply of a catalog release as the app follows it: the bridge's apply
/// that core writes the apply's steps to, and the step the apply waits on,
/// which the step line reads. Shots and tests use a fixed source, which has
/// no bridge apply.
protocol CatalogApplySource: AnyObject, Sendable {
    /// The apply that core writes the steps to; `nil` in shots and tests,
    /// which run without the bridge.
    var bridgeApply: BridgeCatalogApply? { get }

    /// The step the apply waits on, now and as it changes; `nil` while it
    /// waits on none.
    func stepValues() -> PulledValues<BridgeCatalogApplyStep?>
}

extension BridgeCatalogApply: CatalogApplySource {
    var bridgeApply: BridgeCatalogApply? { self }

    func stepValues() -> PulledValues<BridgeCatalogApplyStep?> {
        PulledValues(subscribeStep())
    }
}
