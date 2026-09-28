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
                VStack(spacing: ThemeSpace.compact) {
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
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                statusLine {
                    if let symbol = StatusTone.warning.symbol {
                        Image(systemName: symbol)
                            .foregroundStyle(StatusTone.warning.color)
                    }
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
            .frame(maxWidth: .infinity, alignment: .leading)
            .notice(.warning)
            .padding(.horizontal, ThemeSpace.edge)
            .padding(.vertical, ThemeSpace.related)
        }
    }

    private func statusLine<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(spacing: ThemeSpace.related) {
            content()
        }
    }

    private func surface<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .padding(.horizontal, ThemeSpace.edge)
            .padding(.vertical, ThemeSpace.related)
            .background(.bar)
    }

    private var cancelButton: some View {
        Button(String(localized: "Cancel")) {
            store.cancel()
        }
        .buttonStyle(.borderless)
    }
}
