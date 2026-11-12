import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("AutomationSettingsTab")

struct AutomationSettingsTab: View {
    @Environment(Automation.self)
    var automation
    @Environment(ConfigStore.self)
    var configStore

    private let copyToPasteboard: @MainActor (String) -> Void

    init(
        copyToPasteboard: @escaping @MainActor (String) -> Void =
            SystemActions.copyToPasteboard
    ) {
        self.copyToPasteboard = copyToPasteboard
    }

    var body: some View {
        AutomationSettingsContent(
            automation: automation,
            configStore: configStore,
            copyToPasteboard: copyToPasteboard
        )
    }
}

// MARK: - AutomationSettingsContent

struct AutomationSettingsContent: View {
    let automation: Automation
    let configStore: ConfigStore
    let copyToPasteboard: @MainActor (String) -> Void

    @State
    private var enabled: Bool
    @State
    private var portText: String
    @State
    private var status: BridgeServerStatus = .disabled
    @State
    private var mutationTask: Task<Void, Never>?
    @State
    private var message: AutomationSettingsMessage?

    private enum TokenAction {
        case copy
        case rotate
    }

    private enum AutomationSettingsMessage {
        case feedback(String)
        case error(DisplayError)
    }

    init(
        automation: Automation,
        configStore: ConfigStore,
        copyToPasteboard: @escaping @MainActor (String) -> Void
    ) {
        self.automation = automation
        self.configStore = configStore
        self.copyToPasteboard = copyToPasteboard
        let mcp = configStore.config.mcp
        _enabled = State(initialValue: mcp.enabled)
        _portText = State(initialValue: String(mcp.port))
    }

    private var isWorking: Bool { mutationTask != nil }

    var body: some View {
        SettingsPane(.automation) {
            form
        }
        .task(id: configStore.config.mcp) {
            syncFromConfig(configStore.config.mcp)
            do {
                status = try await automation.getMcpServerStatus()
            }
            catch is CancellationError {
                logger.debug("MCP status task cancelled")
            }
            catch {
                showError(error)
            }
        }
        .onDisappear { mutationTask?.cancel() }
    }

    private var form: some View {
        Form {
            Section {
                Toggle(
                    "Enable MCP server",
                    isOn: Binding(
                        get: { enabled },
                        set: { setEnabled($0) }
                    )
                )
                MacFieldRow("Port") {
                    TextField("Port", text: $portText)
                        .labelsHidden()
                        .boxedTextField()
                        .frame(width: MacPortField.width)
                        .onChange(of: portText) { _, typed in
                            let digits = MacPortField.digitsOnly(typed)
                            if digits != typed { portText = digits }
                        }
                        .onSubmit(applyPort)
                    Button("Save", action: applyPort)
                        .disabled(isWorking)
                }
                statusRow
                if case .error(let error) = status {
                    Notice(
                        title: error.mcpSummary,
                        advice: nil,
                        error: DisplayError(
                            line: error.mcpSummary,
                            detail: error.detail
                        )
                    )
                    .settingsNoticeRow()
                }
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
                HStack(spacing: ThemeSpace.related) {
                    Button("Copy token", action: copyToken)
                    Button("Rotate token", action: rotateToken)
                }
                .disabled(isWorking)
            }
        }
        .formStyle(.settings)
    }

    private func syncFromConfig(_ mcp: BridgeMcpConfig) {
        enabled = mcp.enabled
        portText = String(mcp.port)
    }

    private func setEnabled(_ target: Bool) {
        enabled = target
        save(enabled: target)
    }

    private func applyPort() {
        save(enabled: enabled)
    }

    private func save(enabled targetEnabled: Bool) {
        mutationTask?.cancel()
        message = nil
        let automation = automation
        let portText = portText
        mutationTask = Task {
            defer { mutationTask = nil }
            do {
                try await automation.setMcpServerConfig(targetEnabled, portText)
                status = try await automation.getMcpServerStatus()
            }
            catch is CancellationError {
                logger.debug("MCP settings update task cancelled")
            }
            catch {
                guard
                    let failure = DisplayError(error)?
                        .rewording({
                            String(
                                localized:
                                    "Could not update MCP settings: \($0)"
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

    private func refreshStatus() {
        mutationTask?.cancel()
        clearErrorMessage()
        let automation = automation
        mutationTask = Task {
            defer { mutationTask = nil }
            do {
                status = try await automation.getMcpServerStatus()
            }
            catch is CancellationError {
                logger.debug("MCP status task cancelled")
            }
            catch {
                showError(error)
            }
        }
    }

    private func showError(_ error: any Error) {
        guard let failure = DisplayError(error) else { return }
        message = .error(failure)
    }

    private func copyToken() {
        let automation = automation
        copyTokenFrom(
            action: .copy,
            tokenProvider: { try await automation.getMcpToken() }
        )
    }

    private func rotateToken() {
        let automation = automation
        copyTokenFrom(
            action: .rotate,
            tokenProvider: {
                let token = try await automation.generateMcpToken()
                try await automation.setMcpToken(token)
                return token
            }
        )
    }

    private func copyTokenFrom(
        action: TokenAction,
        tokenProvider: @escaping @Sendable () async throws -> String
    ) {
        mutationTask?.cancel()
        message = nil
        mutationTask = Task {
            defer { mutationTask = nil }
            do {
                let token = try await tokenProvider()
                copyToPasteboard(token)
                let feedbackText =
                    switch action {
                    case .copy: String(localized: "Token copied")
                    case .rotate: String(localized: "Token rotated and copied")
                    }
                message = .feedback(feedbackText)
            }
            catch is CancellationError {
                logger.debug("MCP token task cancelled")
            }
            catch {
                guard let shown = DisplayError(error) else { return }
                switch action {
                case .copy:
                    message = .error(
                        shown.rewording {
                            String(localized: "Could not copy MCP token: \($0)")
                        }
                    )
                case .rotate:
                    message = .error(
                        shown.rewording {
                            String(
                                localized: "Could not rotate MCP token: \($0)"
                            )
                        }
                    )
                }
            }
        }
    }
}

/// The Status row.
extension AutomationSettingsContent {
    @ViewBuilder
    private var statusRow: some View {
        LabeledContent("Status") {
            HStack(spacing: ThemeSpace.group) {
                switch status {
                case .disabled:
                    Text("Disabled")
                        .foregroundStyle(.secondary)
                case .running(let url):
                    Text(url)
                        .textSelection(.enabled)
                case .error:
                    Text("Not running")
                        .foregroundStyle(.secondary)
                }
                Button("Refresh", action: refreshStatus)
                    .disabled(isWorking)
            }
        }
    }
}

#if DEBUG
    #Preview("Automation") {
        AutomationSettingsContent(
            automation: Automation(
                setMcpServerConfig: { _, _ in },
                getMcpServerStatus: {
                    .running(url: "http://127.0.0.1:47777/mcp")
                },
                getMcpToken: { "token" },
                generateMcpToken: { "replacement-token" },
                setMcpToken: { _ in }
            ),
            configStore: PreviewData.configStore(),
            copyToPasteboard: { _ in }
        )
        .frame(width: 500, height: 300)
    }
#endif

extension BridgeServerError {
    fileprivate var mcpSummary: String {
        switch self {
        case .invalidConfig:
            String(localized: "Invalid MCP server configuration")
        case .credentialUnavailable:
            String(localized: "MCP token unavailable")
        case .portInUse, .bindFailed:
            String(localized: "MCP server could not start")
        case .serverFailed:
            String(localized: "MCP server stopped")
        }
    }
}
