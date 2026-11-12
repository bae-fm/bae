import BaeKit
import SwiftUI

struct SubsonicSettingsTab: View {
    @Environment(SubsonicServer.self)
    private var subsonic
    @Environment(SubsonicStore.self)
    private var store
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var names = NetworkNames.current()

    var body: some View {
        SubsonicSettingsContent(
            subsonic: subsonic,
            view: store.view,
            config: configStore.config.subsonic,
            names: names,
            copy: SystemActions.copyToPasteboard,
            showError: { uiStore.showError($0) }
        )
        .task(id: store.view.networks) {
            names = NetworkNames.current()
        }
    }
}

// MARK: - SubsonicSettingsContent

/// The Subsonic pane: the switch is what the person wants, the line under it
/// is what the server is doing, and why it is not running is shown where it
/// is fixed. Every field saves on Return or on leaving it.
struct SubsonicSettingsContent: View {
    let subsonic: SubsonicServer
    /// What the server is doing, the networks to choose from and whether a
    /// password is saved.
    let view: BridgeSubsonicView
    /// What the person set.
    let config: BridgeSubsonicConfig
    let names: NetworkNames
    let copy: @MainActor (String) -> Void
    let showError: @MainActor (any Error) -> Void

    /// Bumped when a password is saved, which empties the field it was typed
    /// in.
    @State
    private var passwordSaves = 0

    /// How far a list of addresses is indented.
    private static let bulletIndent: CGFloat = 16

    /// The username and password fields' width.
    private static let credentialFieldWidth: CGFloat = 180

    var body: some View {
        SettingsPane(.subsonic) {
            form
        }
    }

    private var form: some View {
        Form {
            Section {
                SettingToggleRow(
                    title: "Subsonic server",
                    note: serverNote,
                    isOn: Binding(
                        get: { config.enabled },
                        set: { enabled in
                            perform { try await subsonic.setEnabled(enabled) }
                        }
                    )
                )
                if let detail = failureDetail {
                    Notice(
                        title: String(
                            localized: "Subsonic server could not start"
                        ),
                        advice: nil,
                        error: DisplayError(
                            line: String(
                                localized: "Subsonic server could not start"
                            ),
                            detail: detail
                        )
                    )
                    .joiningRowAbove()
                    .settingsNoticeRow()
                }
                networkRow
                portRow
            }
            Section {
                usernameRow
                passwordRow
            } header: {
                Text("Sign-in")
            } footer: {
                Text(
                    "Subsonic apps sign in with this username and password. The password is kept in this Mac’s keychain."
                )
            }
        }
        .formStyle(.settings)
    }

    // MARK: Rows

    private var portRow: some View {
        SubsonicFieldRow(title: "Port") {
            if let problem = portProblem {
                DangerNote(problem)
            }
        } control: {
            MacPortField(
                value: String(config.port),
                invalid: portProblem != nil,
                onCommit: { text in
                    await save { try await subsonic.setPort(UInt32(text) ?? 0) }
                }
            )
        }
    }

    private var usernameRow: some View {
        SubsonicFieldRow(title: "Username") {
            if causes.contains(.missingUsername) {
                DangerNote("Required to start the server")
            }
        } control: {
            CommittedTextField(
                placeholder: "",
                value: config.username,
                fillsWidth: true,
                invalid: causes.contains(.missingUsername),
                onCommit: { text in
                    await save { try await subsonic.setUsername(text) }
                }
            )
            .frame(width: Self.credentialFieldWidth)
            .accessibilityLabel(Text("Username"))
        }
    }

    private var passwordRow: some View {
        SubsonicFieldRow(title: "Password") {
            if causes.contains(.missingPassword) {
                DangerNote("Required to start the server")
            }
        } control: {
            CommittedTextField(
                placeholder: view.passwordSaved ? "••••••••" : "",
                value: "",
                fillsWidth: true,
                placeholderRole: .emptyMark,
                invalid: causes.contains(.missingPassword),
                secret: true,
                onCommit: { text in
                    await save {
                        try await subsonic.setPassword(text)
                        passwordSaves += 1
                    }
                }
            )
            .id(passwordSaves)
            .frame(width: Self.credentialFieldWidth)
            .accessibilityLabel(Text("Password"))
        }
    }

    /// Where apps connect while the server runs: one address after the words,
    /// or a bullet for each of several.
    @ViewBuilder
    private var connectNote: some View {
        if case .running(let addresses) = view.status {
            if addresses.count == 1, let address = addresses.first {
                HStack(spacing: ThemeSpace.inline) {
                    Text("Apps connect at")
                    copyText(address)
                }
                .themeText(.detail)
                .foregroundStyle(.secondary)
            }
            else if !addresses.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Apps connect at:")
                    // A list under the words: the bullets hang in the 16
                    // points the addresses are indented by.
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(addresses, id: \.url) { address in
                            HStack(
                                alignment: .firstTextBaseline,
                                spacing: ThemeSpace.inline
                            ) {
                                Text(verbatim: "•")
                                    .frame(
                                        width: Self.bulletIndent
                                            - ThemeSpace.inline,
                                        alignment: .trailing
                                    )
                                copyText(address)
                            }
                        }
                    }
                    .padding(.top, 2)
                }
                .themeText(.detail)
                .foregroundStyle(.secondary)
            }
        }
    }

    private func copyText(_ address: BridgeServedAddress) -> HoverCopyText {
        HoverCopyText(
            text: address.url,
            after: address.interface.map { "(\(names.name(of: $0)))" },
            copy: copy
        )
    }

    // MARK: What the server says

    private var causes: [BridgeSubsonicNotRunning] {
        if case .notRunning(let causes, _) = view.status {
            causes
        }
        else {
            []
        }
    }

    /// The line under the switch.
    private var serverNote: LocalizedStringKey {
        switch view.status {
        case .off:
            return "Stream your library using a Subsonic app"
        case .running:
            return "Running"
        case .notRunning:
            for case .networkGone(let interface) in causes {
                let name = names.name(of: interface)
                return "Not running: \(name) is not connected"
            }
            return "Not running"
        }
    }

    /// What the system said, when the server could not start for a reason
    /// that is not for a field or the network to show.
    private var failureDetail: String? {
        for case .failed(let detail) in causes { return detail }
        return nil
    }

    private var portProblem: LocalizedStringKey? {
        if causes.contains(.portTaken) {
            return "Another app is using this port"
        }
        if causes.contains(.portOutOfRange) {
            return "Use a number from 1 to 65535"
        }
        return nil
    }

    // MARK: Saving

    private func perform(_ save: @escaping @Sendable () async throws -> Void) {
        Task { await self.save(save) }
    }

    private func save(_ save: () async throws -> Void) async {
        do { try await save() }
        catch { showError(error) }
    }
}

/// The Available on row and the choice it makes.
extension SubsonicSettingsContent {
    private var networkRow: some View {
        SubsonicFieldRow(title: "Available on") {
            connectNote
        } control: {
            MacPopUpButton("Available on", selection: networkChoice) {
                MacPopUpOption(
                    "This Mac only",
                    value: NetworkChoice.thisMachine
                )
                if !view.networks.isEmpty {
                    MacMenuDivider()
                    for network in view.networks {
                        let name = names.name(of: network.interface)
                        MacPopUpOption(
                            verbatim: networkTitle(network, name: name),
                            value: NetworkChoice.interface(network.interface),
                            closedTitle: name
                        )
                    }
                }
                MacMenuDivider()
                MacPopUpOption("Every network", value: NetworkChoice.every)
            }
            .fixedSize()
        }
    }

    // MARK: The network

    fileprivate enum NetworkChoice: Hashable {
        case thisMachine
        case interface(String)
        case every
    }

    private var networkChoice: Binding<NetworkChoice> {
        Binding(
            get: {
                switch config.network {
                case .thisMachine: .thisMachine
                case .interface(let interface): .interface(interface)
                case .every: .every
                }
            },
            set: { choice in
                let network: BridgeSubsonicNetwork =
                    switch choice {
                    case .thisMachine: .thisMachine
                    case .interface(let interface):
                        .interface(interface: interface)
                    case .every: .every
                    }
                perform { try await subsonic.setNetwork(network) }
            }
        )
    }

    private func networkTitle(_ network: BridgeNetworkChoice, name: String)
        -> String
    {
        guard let address = network.address else {
            return String(localized: "\(name) · not connected")
        }
        return FactList.joined([name, address])
    }
}

/// A settings row with a second line under its title, and a control at the
/// trailing edge.
private struct SubsonicFieldRow<Second: View, Control: View>: View {
    let title: LocalizedStringKey
    @ViewBuilder
    let second: Second
    @ViewBuilder
    let control: Control

    var body: some View {
        HStack(alignment: .center, spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(title)
                second
            }
            // The words keep their line while the control beside them takes
            // what is left.
            .layoutPriority(1)
            Spacer(minLength: 0)
            control
        }
    }
}

/// A row's second line when something is wrong at that row.
private struct DangerNote: View {
    let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .themeText(.detail)
            .foregroundStyle(Theme.danger)
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
            view: BridgeSubsonicView(
                status: .running(
                    addresses: [
                        BridgeServedAddress(
                            url: "http://localhost:4533",
                            interface: nil
                        )
                    ]
                ),
                networks: [],
                passwordSaved: true
            ),
            config: PreviewData.configStore().config.subsonic,
            names: NetworkNames(),
            copy: { _ in },
            showError: { _ in }
        )
        .frame(width: 500, height: 400)
    }
#endif
