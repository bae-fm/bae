import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("SubsonicSettingsTab")

struct SubsonicSettingsTab: View {
    @Environment(SubsonicServer.self)
    var subsonic
    @Environment(ConfigStore.self)
    var configStore

    var body: some View {
        SubsonicSettingsContent(
            subsonic: subsonic,
            configStore: configStore
        )
    }
}

// MARK: - SubsonicSettingsContent

struct SubsonicSettingsContent: View {
    let subsonic: SubsonicServer
    let configStore: ConfigStore

    @State
    private var enabled = false
    @State
    private var portText = ""
    @State
    private var username = ""
    @State
    private var password = ""
    /// Whether the server binds a network-reachable address. On maps to
    /// `0.0.0.0` (reachable from other devices on the LAN), off to `127.0.0.1`
    /// (this machine only). The raw IP never reaches the user.
    @State
    private var allowNetwork = false
    @State
    private var status: BridgeSubsonicServerStatus = .disabled
    @State
    private var mutationTask: Task<Void, Never>?
    @State
    private var message: SubsonicSettingsMessage?

    private enum SubsonicSettingsMessage {
        case feedback(String)
        case error(String)

        var text: String {
            switch self {
            case .feedback(let text), .error(let text): text
            }
        }

        var style: Color {
            switch self {
            case .feedback: .secondary
            case .error: .red
            }
        }
    }

    private var isWorking: Bool { mutationTask != nil }

    var body: some View {
        Form {
            Section {
                Toggle(
                    "Enable Subsonic server",
                    isOn: Binding(
                        get: { enabled },
                        set: { setEnabled($0) }
                    )
                )
                LabeledContent("Port") {
                    HStack(spacing: 8) {
                        TextField("Port", text: $portText)
                            .frame(width: 88)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit(applyConfig)
                        Button("Save", action: applyConfig)
                            .disabled(isWorking)
                    }
                }
                LabeledContent("Username") {
                    HStack(spacing: 8) {
                        TextField("Username", text: $username)
                            .frame(width: 180)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit(applyConfig)
                        Button("Save", action: applyConfig)
                            .disabled(isWorking)
                    }
                }
                Toggle(
                    "Allow connections from other devices on your network",
                    isOn: Binding(
                        get: { allowNetwork },
                        set: { setAllowNetwork($0) }
                    )
                )
                statusRow
                if let message {
                    Text(message.text)
                        .font(.callout)
                        .foregroundStyle(message.style)
                }
            }
            Section {
                LabeledContent("Password") {
                    HStack(spacing: 8) {
                        SecureField("Password", text: $password)
                            .frame(width: 180)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit(savePassword)
                        Button("Save", action: savePassword)
                            .disabled(isWorking)
                    }
                }
            } footer: {
                Text(
                    "The password is stored in the keyring, not in the config file. Clients authenticate with the username and password."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .task(id: configStore.config.subsonic) {
            syncFromConfig(configStore.config.subsonic)
            status = await subsonic.getServerStatus()
        }
        .onDisappear { mutationTask?.cancel() }
    }

    @ViewBuilder
    private var statusRow: some View {
        LabeledContent("Status") {
            HStack(spacing: 8) {
                switch status {
                case .disabled:
                    Text("Disabled")
                        .foregroundStyle(.secondary)
                case .running(let url):
                    Text(url)
                        .textSelection(.enabled)
                case .error(let error):
                    VStack(alignment: .leading, spacing: 2) {
                        Text(error.localizedSummary)
                            .foregroundStyle(.red)
                        Text(error.detail)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                }
                if isWorking {
                    ProgressView().controlSize(.small)
                }
                Button("Refresh", action: refreshStatus)
                    .disabled(isWorking)
            }
        }
    }

    private func syncFromConfig(_ config: BridgeSubsonicConfig) {
        enabled = config.enabled
        portText = String(config.port)
        username = config.username
        // Anything other than pure loopback means the server is reachable from
        // the network, so the toggle reads on.
        allowNetwork = config.bindAddress != "127.0.0.1"
    }

    private func setEnabled(_ target: Bool) {
        enabled = target
        applyConfig()
    }

    private func setAllowNetwork(_ target: Bool) {
        allowNetwork = target
        applyConfig()
    }

    private func applyConfig() {
        mutationTask?.cancel()
        message = nil
        let subsonic = subsonic
        let targetEnabled = enabled
        let portText = portText
        let username = username
        let bindAddress = allowNetwork ? "0.0.0.0" : "127.0.0.1"
        mutationTask = Task {
            defer { mutationTask = nil }
            do {
                try await subsonic.setServerConfig(
                    targetEnabled,
                    portText,
                    username,
                    bindAddress
                )
                status = await subsonic.getServerStatus()
            }
            catch is CancellationError {
                logger.debug("Subsonic settings update task cancelled")
            }
            catch {
                message = .error(
                    String(
                        localized:
                            "Couldn't update Subsonic settings: \(error.displayLine)"
                    )
                )
            }
        }
    }

    private func savePassword() {
        mutationTask?.cancel()
        message = nil
        let subsonic = subsonic
        let password = password
        mutationTask = Task {
            defer { mutationTask = nil }
            do {
                try await subsonic.setPassword(password)
                status = await subsonic.getServerStatus()
                message = .feedback(String(localized: "Password saved"))
            }
            catch is CancellationError {
                logger.debug("Subsonic password task cancelled")
            }
            catch {
                message = .error(
                    String(
                        localized:
                            "Couldn't save Subsonic password: \(error.displayLine)"
                    )
                )
            }
        }
    }

    private func clearErrorMessage() {
        if case .error = message {
            message = nil
        }
    }

    private func refreshStatus() {
        mutationTask?.cancel()
        clearErrorMessage()
        let subsonic = subsonic
        mutationTask = Task {
            defer { mutationTask = nil }
            status = await subsonic.getServerStatus()
        }
    }
}

#if DEBUG
    #Preview("Subsonic") {
        SubsonicSettingsContent(
            subsonic: SubsonicServer(
                setServerConfig: { _, _, _, _ in },
                getServerStatus: {
                    .running(url: "http://127.0.0.1:4533/rest")
                },
                setPassword: { _ in }
            ),
            configStore: PreviewData.configStore
        )
        .frame(width: 500, height: 300)
    }
#endif

extension BridgeSubsonicServerError {
    fileprivate var localizedSummary: String {
        switch self {
        case .invalidConfig:
            String(localized: "Invalid Subsonic server configuration")
        case .credentialUnavailable:
            String(localized: "Subsonic credential unavailable")
        case .bindFailed:
            String(localized: "Subsonic server could not start")
        case .serverFailed:
            String(localized: "Subsonic server stopped")
        }
    }

    fileprivate var detail: String {
        switch self {
        case .invalidConfig(let detail),
            .credentialUnavailable(let detail),
            .bindFailed(let detail),
            .serverFailed(let detail):
            detail
        }
    }
}
