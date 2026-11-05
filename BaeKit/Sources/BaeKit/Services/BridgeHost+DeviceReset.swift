import Observation

extension BridgeHost {
    /// Load a closed library from storage again under a fresh device id,
    /// returning its open store and the error a partly finished reset
    /// reported. The caller closed the library and showed the reset's warning
    /// before calling; nothing is shown after. The reset writes each step of
    /// loading the library into `setup`'s stream as it runs.
    public func resetDevice(
        libraryId: String,
        deviceName: String,
        setup: BridgeLibrarySetup
    ) async throws -> (library: OpenedLibrary, warning: BridgeError?) {
        let reset = try prepareDeviceReset(
            libraryId: libraryId,
            deviceName: deviceName
        )
        switch try await reset.confirm(setup: setup) {
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

/// One device reset as the app follows it: the setup the reset is handed,
/// and where it stands, read off that setup's stream. Make one for each
/// reset and stop it once the reset ends. Its stream is pulled on `host`'s
/// runtime.
@MainActor
@Observable
public final class DeviceResetSteps {
    /// Where the reset stands; nil until it reports a step.
    public var status: BridgeBootstrapStatus? { followed.value?.status }
    @ObservationIgnored
    public let setup: BridgeLibrarySetup
    @ObservationIgnored
    private let followed: FollowedValue<BridgeBootstrapState>
    @ObservationIgnored
    private let feed: LatestFeed

    public init(host: BridgeHost) {
        setup = BridgeLibrarySetup(host: host)
        let stream = LatestStream<BridgeBootstrapState>.bootstrapState()
        feed = stream.feed(from: setup)
        followed = FollowedValue(stream.only)
    }

    /// Stop reading the stream; the status stays as it last was.
    public func stop() {
        followed.stop()
        feed.cancel()
    }
}
