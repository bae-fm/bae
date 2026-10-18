import SwiftUI

public struct ArtworkLoadingBanner: View {
    @Environment(ArtworkLoadingStore.self)
    private var store
    public init() {}
    public var body: some View {
        VStack {
            switch store.status {
            case .idle: EmptyView()
            case .downloading(let progress):
                HStack {
                    Text(localizedCoreString("core.artwork_cache.downloading"))
                    Spacer()
                    Text(progress.bytesText).monospacedDigit()
                    Button("Cancel") { Task { await store.cancel() } }
                        .buttonStyle(.borderless)
                }
                ProgressView(
                    value: Double(progress.bytesCached),
                    total: Double(progress.bytesTotal)
                )
            case .cancelled(let progress):
                HStack {
                    Text(localizedCoreString("core.artwork_cache.cancelled"))
                    Spacer()
                    Text(progress.bytesText).monospacedDigit()
                }
            }
            if let error = store.error { ErrorDetailDisclosure(error: error) }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .background(.bar)
    }
}
