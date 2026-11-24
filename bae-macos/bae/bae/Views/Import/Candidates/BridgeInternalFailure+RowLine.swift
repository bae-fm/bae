import BaeKit

extension BridgeInternalFailure {
    /// What a list row says when bae broke on its own side: the category,
    /// since the error chain is for the folder's pane.
    var rowLine: String {
        localizedCoreString("core.error.category.internal")
    }
}
