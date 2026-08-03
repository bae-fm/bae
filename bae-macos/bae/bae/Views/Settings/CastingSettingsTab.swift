import BaeKit
import SwiftUI

/// What flipping the casting toggle should do.
enum CastToggleAction: Equatable {
    /// Write the setting straight through.
    case apply(Bool)
    /// Turning casting off would end the session on this device — ask first.
    case confirmDisconnect(device: String)
}

/// The "Casting" settings tab: one toggle for the whole feature. Core is what
/// the toggle actually gates — while off it browses no network and starts no
/// session — so this tab only writes the setting and warns before a write that
/// would cut a session short.
struct CastingSettingsTab: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    /// The device an unconfirmed "turn casting off" would disconnect from.
    @State
    private var pendingDisconnect: String?

    /// Turning casting off mid-session ends it, so that one case asks first;
    /// every other flip writes straight through.
    static func toggleAction(
        enabled: Bool,
        castingDeviceName: String?
    ) -> CastToggleAction {
        guard !enabled, let device = castingDeviceName else {
            return .apply(enabled)
        }
        return .confirmDisconnect(device: device)
    }

    var body: some View {
        Form {
            Section {
                Toggle("Enable casting", isOn: enabledBinding)
            } footer: {
                Text(
                    "Plays to Cast, AirPlay, and UPnP receivers on your network. While off, bae does not look for devices."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
        .alert(
            "Turn off casting?",
            isPresented: confirmingDisconnect,
            presenting: pendingDisconnect
        ) { _ in
            Button("Turn Off", role: .destructive) { setEnabled(false) }
            Button("Cancel", role: .cancel) {}
        } message: { device in
            Text("This will stop casting to \(device).")
        }
    }

    /// Reads the persisted setting and writes through the bridge — the config
    /// invalidation is what moves the switch, so a refused or cancelled flip
    /// leaves it where it was with nothing to undo.
    private var enabledBinding: Binding<Bool> {
        Binding(
            get: { configStore.config.castEnabled },
            set: { enabled in
                switch Self.toggleAction(
                    enabled: enabled,
                    castingDeviceName: castStore.castingDeviceName
                ) {
                case .apply(let enabled): setEnabled(enabled)
                case .confirmDisconnect(let device): pendingDisconnect = device
                }
            }
        )
    }

    private var confirmingDisconnect: Binding<Bool> {
        Binding(
            get: { pendingDisconnect != nil },
            set: { presented in
                if !presented { pendingDisconnect = nil }
            }
        )
    }

    private func setEnabled(_ enabled: Bool) {
        do {
            try cast.setEnabled(enabled)
        }
        catch {
            uiStore.showError(error)
        }
    }
}

#if DEBUG
    #Preview("Casting Settings") {
        CastingSettingsTab()
            .environment(Cast.stub)
            .environment(PreviewData.castStore)
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    castEnabled: true
                )
            )
            .environment(UiStore())
            .frame(width: 500, height: 300)
    }
#endif
