import Foundation

/// The calls Add Device's pairing makes of core: start it, answer a new
/// device's request, and end it when the sheet that showed its code closes.
/// The pairing's status is followed through `MembershipStore.followAddDevice`.
public struct AddDeviceCalls: Sendable {
    /// Start a pairing: its id and codes.
    public let start: @Sendable () async throws -> BridgeAddDevice
    public let approve: @Sendable (BridgeAddDeviceRequest) async throws -> Void
    public let decline: @Sendable (BridgeAddDeviceRequest) async throws -> Void
    /// End the pairing by its id, whatever its status.
    public let end: @Sendable (String) async throws -> Void

    public init(
        start: @escaping @Sendable () async throws -> BridgeAddDevice = {
            throw StubError.notImplemented
        },
        approve:
            @escaping @Sendable (BridgeAddDeviceRequest) async throws -> Void =
            {
                _ in throw StubError.notImplemented
            },
        decline:
            @escaping @Sendable (BridgeAddDeviceRequest) async throws -> Void =
            {
                _ in throw StubError.notImplemented
            },
        end: @escaping @Sendable (String) async throws -> Void = { _ in
            throw StubError.notImplemented
        }
    ) {
        self.start = start
        self.approve = approve
        self.decline = decline
        self.end = end
    }

    /// The production wiring over the open library's handle.
    public init(handle: any AppHandleProtocol) {
        self.init(
            start: { try await handle.addDevice() },
            approve: { try await handle.approveAddDevice(request: $0) },
            decline: { try await handle.declineAddDevice(request: $0) },
            end: { try await handle.endAddDevice(pairing: $0) }
        )
    }
}
