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
        return Self.reset(try await reset.confirm(setup: setup))
    }

    /// Finish a reset of a library's copy on this device that stopped
    /// partway, from what core kept: the device name it began with, and no
    /// warning, which the person accepted when it began. A cancel can't stop
    /// a reset under way, so none is offered. Returns as `resetDevice` does.
    public func finishReset(
        libraryId: String,
        setup: BridgeLibrarySetup
    ) async throws -> (library: OpenedLibrary, warning: BridgeError?) {
        Self.reset(
            try await resumeReplacement(
                libraryId: libraryId,
                by: .reset,
                setup: setup,
                cancellation: BootstrapCancellation()
            )
        )
    }

    /// A reset's open store, and the error a partly finished reset reported.
    private static func reset(_ result: BridgeBootstrapResult) -> (
        library: OpenedLibrary, warning: BridgeError?
    ) {
        switch result {
        case .opened(let library):
            return (library, nil)
        case .published(let library, let warning),
            .profileRequired(let library, let warning):
            return (library, warning)
        case .savedCodeRemoved, .declined, .joinRequestSent, .callUnderWay:
            preconditionFailure(
                "a device reset reads no code and runs on a library this device lists"
            )
        }
    }
}

/// One device reset as the app follows it: the setup the reset is handed,
/// where it stands, and the counted step it runs. Make one for each reset
/// and stop it once the reset ends. Its states and steps are pulled on
/// `host`'s runtime.
@MainActor
@Observable
public final class DeviceResetSteps {
    /// Where the reset stands; nil until it says.
    public var status: BridgeBootstrapStatus? { following.state?.status }
    /// The counted step the reset runs, for the line that draws it.
    public var steps: LatestSource<BridgeBootstrapStep> { following.steps }
    public var setup: BridgeLibrarySetup { following.setup }
    private let following: BootstrapFollowing

    public init(host: BridgeHost) {
        following = BootstrapFollowing(setup: BridgeLibrarySetup(host: host))
    }

    /// Stop following; the status stays as it last was.
    public func stop() {
        following.stop()
    }
}
