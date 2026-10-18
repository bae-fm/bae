extension BridgeProviderSignOut {
    /// What the person still has to do in the provider after removing a
    /// device, naming the provider when the step is in its account.
    public var text: String {
        let line = localizedCoreString(
            bridgeProviderSignOutKey(instruction: self)
        )
        if case .removeAppAccess(let provider) = self {
            return "\(provider.displayName): \(line)"
        }
        return line
    }
}
