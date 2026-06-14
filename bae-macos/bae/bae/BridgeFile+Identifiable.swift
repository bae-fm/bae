// BridgeFile carries a stable `id`, so it satisfies Identifiable directly.
// Conforming lets SwiftUI's `Table` and other identity-based views key on it
// without a separate `id:` keypath.
extension BridgeFile: Identifiable {}
