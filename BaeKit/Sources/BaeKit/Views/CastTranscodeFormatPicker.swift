import SwiftUI

/// What a track a Cast or UPnP device can't play directly is converted to:
/// MP3, or lossless WAV. Core decides per device flavor whether the choice
/// applies (a UPnP device gets MP3 either way), so each platform's footer says
/// what its devices do. Only drawn while casting is on — with casting off
/// nothing is converted for a device.
@MainActor
public struct CastTranscodeFormatPicker: View {
    private let configStore: ConfigStore
    private let setFormat:
        @Sendable (BridgeCastTranscodeFormat) async throws -> Void
    /// Takes the error, not a rendered line, like `SidePauseCountdownPicker`.
    private let showError: @MainActor (any Error) -> Void

    public init(
        configStore: ConfigStore,
        setFormat:
            @escaping @Sendable (BridgeCastTranscodeFormat) async throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.configStore = configStore
        self.setFormat = setFormat
        self.showError = showError
    }

    /// Whether settings draw the picker for `config`: only while casting is on.
    public static func isShown(for config: Config) -> Bool {
        config.castEnabled
    }

    public var body: some View {
        if Self.isShown(for: configStore.config) {
            Picker("Convert to", selection: binding) {
                Text(verbatim: "MP3").tag(BridgeCastTranscodeFormat.mp3)
                Text("WAV (lossless)").tag(BridgeCastTranscodeFormat.wav)
            }
        }
    }

    private var binding: Binding<BridgeCastTranscodeFormat> {
        Binding(
            get: { configStore.config.castTranscodeFormat },
            set: { format in
                // The write is a durable file replace, awaited off the main
                // thread; the config mirror re-renders once it lands.
                Task {
                    do {
                        try await setFormat(format)
                    }
                    catch {
                        showError(error)
                    }
                }
            }
        )
    }
}
