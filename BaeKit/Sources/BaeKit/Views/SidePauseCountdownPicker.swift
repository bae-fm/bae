import SwiftUI

/// The countdown choice under the pause-between-sides toggle: whether a side
/// or disc pause ends on its own, and after how long. Only drawn while pausing
/// between sides is on — with pausing off there is no pause to count down.
@MainActor
public struct SidePauseCountdownPicker: View {
    private let configStore: ConfigStore
    private let setCountdown:
        @Sendable (BridgeSidePauseCountdown) async throws -> Void
    /// Takes the error, not a rendered line, like `PauseBetweenSidesToggle`.
    private let showError: @MainActor (any Error) -> Void

    public init(
        configStore: ConfigStore,
        setCountdown:
            @escaping @Sendable (BridgeSidePauseCountdown) async throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.configStore = configStore
        self.setCountdown = setCountdown
        self.showError = showError
    }

    /// Whether settings draw the picker for `config`: only while pausing
    /// between sides is on.
    public static func isShown(for config: Config) -> Bool {
        config.pauseBetweenSides
    }

    public var body: some View {
        if Self.isShown(for: configStore.config) {
            Picker("Countdown", selection: binding) {
                ForEach(BridgeSidePauseCountdown.offered, id: \.self) {
                    choice in
                    choiceLabel(choice).tag(choice)
                }
            }
        }
    }

    private func choiceLabel(_ choice: BridgeSidePauseCountdown) -> Text {
        guard let seconds = choice.seconds else {
            return Text("Off")
        }
        return Text(verbatim: BridgeSidePauseCountdown.label(seconds: seconds))
    }

    private var binding: Binding<BridgeSidePauseCountdown> {
        Binding(
            get: { configStore.config.sidePauseCountdown },
            set: { countdown in
                // The write is a durable file replace, awaited off the main
                // thread; the config mirror re-renders once it lands.
                Task {
                    do {
                        try await setCountdown(countdown)
                    }
                    catch {
                        showError(error)
                    }
                }
            }
        )
    }
}

extension BridgeSidePauseCountdown {
    /// Every choice, in the order settings offer them.
    public static let offered: [BridgeSidePauseCountdown] = [
        .off, .seconds5, .seconds15, .seconds30, .seconds45, .seconds60,
    ]

    /// How long the pause lasts, or `nil` when it waits for Play.
    public var seconds: Int? {
        switch self {
        case .off: nil
        case .seconds5: 5
        case .seconds15: 15
        case .seconds30: 30
        case .seconds45: 45
        case .seconds60: 60
        }
    }

    /// A length in words for the current locale ("5 seconds"), from
    /// Foundation's duration formatter, so every locale gets its own plural.
    public static func label(seconds: Int, locale: Locale = .current) -> String
    {
        Duration.seconds(seconds)
            .formatted(
                .units(allowed: [.seconds], width: .wide).locale(locale)
            )
    }
}
