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
    /// Whether the server listens on `0.0.0.0` (the local network) rather than
    /// `127.0.0.1` (this machine only).
    @State
    private var allowNetwork = false
    @State
    private var mutationTask: Task<Void, Never>?
    @State
    private var message: SubsonicSettingsMessage?

    private enum SubsonicSettingsMessage {
        case feedback(String)
        case error(DisplayError)
    }

    /// The username and password fields' width.
    private static let credentialFieldWidth: CGFloat = 180

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
                LabeledFieldRow("Port") {
                    TextField("Port", text: $portText)
                        .labelsHidden()
                        .boxedTextField()
                        .frame(width: 88)
                        .onSubmit(applyConfig)
                    Button("Save", action: applyConfig)
                        .disabled(isWorking)
                }
                LabeledFieldRow("Username") {
                    TextField("Username", text: $username)
                        .labelsHidden()
                        .boxedTextField()
                        .frame(width: Self.credentialFieldWidth)
                        .onSubmit(applyConfig)
                    Button("Save", action: applyConfig)
                        .disabled(isWorking)
                }
                Toggle(
                    "Allow connections from other devices on your network",
                    isOn: Binding(
                        get: { allowNetwork },
                        set: { setAllowNetwork($0) }
                    )
                )
                switch message {
                case .feedback(let text):
                    Text(text)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                case .error(let failure):
                    Notice(error: failure)
                case nil:
                    EmptyView()
                }
            }
            Section {
                LabeledFieldRow("Password") {
                    SecureField("Password", text: $password)
                        .labelsHidden()
                        .boxedTextField()
                        .frame(width: Self.credentialFieldWidth)
                        .onSubmit(savePassword)
                    Button("Save", action: savePassword)
                        .disabled(isWorking)
                }
            } footer: {
                Text(
                    "The password is kept in this Mac’s keychain. Clients sign in with the username and password."
                )
                .fieldNote()
            }
        }
        .formStyle(.grouped)
        .task(id: configStore.config.subsonic) {
            syncFromConfig(configStore.config.subsonic)
        }
        .onDisappear { mutationTask?.cancel() }
    }

    private func syncFromConfig(_ config: BridgeSubsonicConfig) {
        enabled = config.enabled
        portText = String(config.port)
        username = config.username
        allowNetwork = config.network != .thisMachine
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
        let network: BridgeSubsonicNetwork =
            allowNetwork ? .every : .thisMachine
        mutationTask = Task {
            defer { mutationTask = nil }
            do {
                guard let port = UInt32(portText) else {
                    throw SubsonicValidationError.invalidPort
                }
                try await subsonic.setEnabled(targetEnabled)
                try await subsonic.setPort(port)
                try await subsonic.setUsername(username)
                try await subsonic.setNetwork(network)
            }
            catch is CancellationError {
                logger.debug("Subsonic settings update task cancelled")
            }
            catch {
                guard
                    let failure = DisplayError(error)?
                        .rewording({
                            String(
                                localized:
                                    "Couldn't update Subsonic settings: \($0)"
                            )
                        })
                else { return }
                message = .error(failure)
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
                message = .feedback(String(localized: "Password saved"))
            }
            catch is CancellationError {
                logger.debug("Subsonic password task cancelled")
            }
            catch {
                guard
                    let failure = DisplayError(error)?
                        .rewording({
                            String(
                                localized:
                                    "Couldn't save Subsonic password: \($0)"
                            )
                        })
                else { return }
                message = .error(failure)
            }
        }
    }

    private func clearErrorMessage() {
        if case .error = message {
            message = nil
        }
    }

    private func showError(_ error: any Error) {
        guard let failure = DisplayError(error) else { return }
        message = .error(failure)
    }
}

#if DEBUG
    #Preview("Subsonic") {
        SubsonicSettingsContent(
            subsonic: SubsonicServer(
                setEnabled: { _ in },
                setPort: { _ in },
                setUsername: { _ in },
                setNetwork: { _ in },
                setPassword: { _ in }
            ),
            configStore: PreviewData.configStore()
        )
        .frame(width: 500, height: 300)
    }
#endif

private enum SubsonicValidationError: LocalizedError {
    case invalidPort

    var errorDescription: String? {
        switch self {
        case .invalidPort:
            String(localized: "Enter a port from 1 to 65535.")
        }
    }
}
