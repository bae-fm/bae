import SwiftUI

/// A small cloud mark on an album whose files are still arriving from another
/// device, naming the device to assistive technologies and on hover. Present
/// only while the album arrives, and invisible rather than removed otherwise,
/// so a card keeps its layout.
public struct AlbumArrivalBadge: View {
    /// The device the album is arriving from, or nil once it has arrived.
    private let device: String?

    public init(device: String?) {
        self.device = device
    }

    public var body: some View {
        let line = device.map(BridgeArrivals.fromLine) ?? ""
        Image(systemName: "icloud.and.arrow.down")
            .themeIcon(.small)
            .foregroundStyle(.secondary)
            .padding(ThemeSpace.inline)
            .background(.thinMaterial, in: Circle())
            .opacity(device == nil ? 0 : 1)
            .accessibilityHidden(device == nil)
            .accessibilityLabel(line)
            .help(line)
    }
}

/// The calm line at the top of a release's detail while some of its files
/// are still arriving: "Arriving from Studio Mac".
public struct ReleaseArrivalLine: View {
    private let releaseId: String

    @Environment(SyncActivityStore.self)
    private var store

    public init(releaseId: String) {
        self.releaseId = releaseId
    }

    public var body: some View {
        if let arrival = store.arrival(releaseId: releaseId) {
            Label(
                BridgeArrivals.fromLine(arrival.deviceName),
                systemImage: "icloud.and.arrow.down"
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
        }
    }
}

extension SyncActivityStore {
    /// The tracks of a release whose audio is still arriving, which can't
    /// play yet.
    public func arrivingTrackIds(releaseId: String) -> Set<String> {
        Set(arrival(releaseId: releaseId)?.trackIds ?? [])
    }
}
