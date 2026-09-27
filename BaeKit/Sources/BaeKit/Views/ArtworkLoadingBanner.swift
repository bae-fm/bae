import SwiftUI

/// Progress fetching the artwork the library keeps locally; the library stays
/// usable meanwhile.
public struct ArtworkLoadingBanner: View {
    @Environment(ArtworkLoadingStore.self)
    private var store

    public init() {}

    public var body: some View {
        switch store.status {
        case .notRunning, .complete:
            EmptyView()
        case .scanning(let titleKey):
            surface {
                statusLine {
                    ProgressView()
                        .controlSize(.small)
                    Text(localizedCoreString(titleKey))
                    Spacer()
                    cancelButton
                }
            }
        case .downloading(let titleKey, let progress):
            surface {
                VStack(spacing: 6) {
                    statusLine {
                        Text(localizedCoreString(titleKey))
                        Spacer()
                        Text(progress.bytesText)
                            .themeText(.detail)
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                        cancelButton
                    }
                    ProgressView(
                        value: Double(progress.bytesDone),
                        total: Double(progress.bytesTotal)
                    )
                    .progressViewStyle(.linear)
                }
            }
        case .cancelled(let titleKey, let progress):
            surface {
                statusLine {
                    Image(systemName: "stop.circle")
                    Text(localizedCoreString(titleKey))
                    Spacer()
                    Text(progress.bytesText)
                        .themeText(.detail)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
        case .failed(let titleKey, let progress, let error):
            surface {
                VStack(alignment: .leading, spacing: 4) {
                    statusLine {
                        Image(systemName: "exclamationmark.triangle")
                            .foregroundStyle(Theme.warning)
                        Text(localizedCoreString(titleKey))
                        Spacer()
                        Text(progress.bytesText)
                            .themeText(.detail)
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                    }
                    Text(error)
                        .themeText(.mono)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
        }
    }

    private func statusLine<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(spacing: 8) {
            content()
        }
    }

    private func surface<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(.bar)
    }

    private var cancelButton: some View {
        Button(String(localized: "Cancel")) {
            store.cancel()
        }
        .buttonStyle(.borderless)
    }
}
