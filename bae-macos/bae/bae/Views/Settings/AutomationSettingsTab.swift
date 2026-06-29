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
    private var enabled = false
    @State
    private var portText = ""
    @State
    private var status: BridgeMcpServerStatus = .disabled
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
                    "Enable MCP server",
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
                            .onSubmit(applyPort)
                        Button("Save", action: applyPort)
                            .disabled(isWorking)
                    }
                }
                statusRow
                if let message {
                    Text(message.text)
                        .font(.callout)
                        .foregroundStyle(message.style)
                }
            }
            Section {
                HStack {
                    Button("Copy token", action: copyToken)
                    Button("Rotate token", action: rotateToken)
                }
                .disabled(isWorking)
            }
        }
        .formStyle(.grouped)
        .task(id: configStore.config.mcp) {
            syncFromConfig(configStore.config.mcp)
            status = await automation.getMcpServerStatus()
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
                status = await automation.getMcpServerStatus()
            }
            catch is CancellationError {
                logger.debug("MCP settings update task cancelled")
            }
            catch {
                message = .error(
                    String(
                        localized:
                            "Couldn't update MCP settings: \(error.localizedDescription)"
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
        let automation = automation
        mutationTask = Task {
            defer { mutationTask = nil }
            status = await automation.getMcpServerStatus()
        }
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
                switch action {
                case .copy:
                    message = .error(
                        String(
                            localized:
                                "Couldn't copy MCP token: \(error.localizedDescription)"
                        )
                    )
                case .rotate:
                    message = .error(
                        String(
                            localized:
                                "Couldn't rotate MCP token: \(error.localizedDescription)"
                        )
                    )
                }
            }
        }
    }
}

#Preview("Automation") {
    AutomationSettingsContent(
        automation: Automation(
            setMcpServerConfig: { _, _ in },
            getMcpServerStatus: { .running(url: "http://127.0.0.1:47777/mcp") },
            getMcpToken: { "token" },
            generateMcpToken: { "replacement-token" },
            setMcpToken: { _ in }
        ),
        configStore: PreviewData.configStore,
        copyToPasteboard: { _ in }
    )
    .frame(width: 500, height: 300)
}

extension BridgeMcpServerError {
    fileprivate var localizedSummary: String {
        switch self {
        case .invalidConfig:
            String(localized: "Invalid MCP server configuration")
        case .tokenUnavailable:
            String(localized: "MCP token unavailable")
        case .bindFailed:
            String(localized: "MCP server could not start")
        case .serverFailed:
            String(localized: "MCP server stopped")
        }
    }

    fileprivate var detail: String {
        switch self {
        case .invalidConfig(let detail),
            .tokenUnavailable(let detail),
            .bindFailed(let detail),
            .serverFailed(let detail):
            detail
        }
    }
}
