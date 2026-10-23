extension BridgeHost {
    /// Load a closed library from storage again under a fresh device id,
    /// returning its open store and the error a partly finished reset
    /// reported. The caller closed the library and showed the reset's warning
    /// before calling; nothing is shown after.
    public func resetDevice(
        libraryId: String,
        deviceName: String
    ) async throws -> (library: OpenedLibrary, warning: BridgeError?) {
        let reset = try prepareDeviceReset(
            libraryId: libraryId,
            deviceName: deviceName
        )
        switch try await reset.confirm() {
        case .opened(let library):
            return (library, nil)
        case .published(let library, let warning),
            .profileRequired(let library, let warning):
            return (library, warning)
        case .savedCodeRemoved, .declined, .joinRequestSent:
            preconditionFailure(
                "a device reset neither reads a saved code nor asks to join"
            )
        }
    }
}
