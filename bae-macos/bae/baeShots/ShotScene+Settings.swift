import BaeKit
import Observation
import SwiftUI

@testable import bae

extension ShotScene {
    /// Settings panes: Playback with pausing between sides on, so its
    /// countdown shows under the switch; Formats, with the file names pattern
    /// at its bottom; Casting on, with the receivers it
    /// found and still looking for the first; and the Library tab of a synced
    /// library, one whose sync failed, and one only on this Mac; then the Sync
    /// page in each state the boards draw.
    static let settingsScenes: [ShotScene] = [
        ShotScene(
            id: "settings-playback",
            size: CGSize(width: 500, height: 300)
        ) {
            ShotStage(
                PlaybackSettingsTab()
                    .environment(Playback.stub())
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            pauseBetweenSides: true
                        )
                    )
                    .environment(UiStore())
            )
        },
        castingSettings("settings-casting", receivers: castPickerReceivers()),
        castingSettings(
            "settings-casting-looking",
            receivers: BridgeCastReceiverList(casting: nil, found: [])
        ),
        ShotScene(
            id: "settings-formats",
            size: CGSize(width: 500, height: 640)
        ) {
            ShotStage(
                FormatsSettingsTab()
                    .environment(PreviewData.configStore())
                    .environment(Outputs.stub())
                    .environment(UiStore())
            )
        },
        ShotScene(
            id: "settings-formats-name-invalid",
            size: CGSize(width: 500, height: 640)
        ) {
            // Typed text a name can't hold turns its field red and takes the
            // preview's place with why, as core answers it.
            ShotStage(
                FormatsSettingsTab()
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            filenamePieces: [
                                .value(value: .title), .text(text: " / "),
                                .value(value: .artist),
                            ],
                            filenamePatternError: .forbiddenCharacter(
                                character: "/"
                            ),
                            folderPieces: [
                                .text(text: ".hidden "), .value(value: .album),
                            ],
                            folderPatternError: .startsWithDot
                        )
                    )
                    .environment(Outputs.stub())
                    .environment(UiStore())
            )
        },
        ShotScene(
            id: "settings-format-name-taken",
            size: CGSize(width: 480, height: 540)
        ) {
            // An AAC format named as the MP3 one already is: the name field
            // turns red and Done is off.
            let aac = BridgeSavePreset(
                id: "aac",
                name: "MP3",
                codec: .aac(bitrateKbps: 256)
            )
            return ShotStage(
                SavePresetEditor(
                    preset: aac,
                    presets: PreviewData.savePresets + [aac],
                    update: { _ in }
                )
            )
        },
        transfersSettings("settings-transfers", uploadsPaused: false),
        transfersSettings(
            "settings-transfers-uploads-paused",
            uploadsPaused: true
        ),
        librarySettings(
            "settings-library",
            storage: PreviewData.previewStorage
        ),
        librarySettings(
            "settings-library-problem",
            storage: PreviewData.previewStorage,
            shown: .previewFailed
        ),
        librarySettings(
            "settings-library-not-synced",
            storage: nil,
            shown: .previewNotSynced
        ),
        ShotScene(
            id: "settings-appearance",
            size: CGSize(width: 500, height: 300)
        ) {
            ShotStage(AppearanceSettingsTab())
        },
        ShotScene(
            id: "settings-about",
            size: CGSize(width: 500, height: 300)
        ) {
            ShotStage(
                AboutSettingsTab(
                    canCheckForUpdates: true,
                    onCheckForUpdates: {}
                )
            )
        },
        settingsSidebar("settings-sidebar", .previewSynced),
        settingsSidebar("settings-sidebar-offline", .previewOffline),
        settingsSidebar("settings-sidebar-problem", .previewFailed),
        automationFailed,
        subsonicPane(
            "settings-subsonic-off",
            config: .previewSubsonic(enabled: false),
            view: .previewSubsonic(.off)
        ),
        subsonicPane(
            "settings-subsonic",
            config: .previewSubsonic(network: .interface(interface: "en0")),
            view: .previewSubsonic(.running(addresses: [.previewWiFi]))
        ),
        subsonicPane(
            "settings-subsonic-every",
            config: .previewSubsonic(network: .every),
            view: .previewSubsonic(
                .running(addresses: [.previewWiFi, .previewTailscale])
            )
        ),
        subsonicPane(
            "settings-subsonic-no-network",
            config: .previewSubsonic(network: .interface(interface: "en0")),
            view: .previewSubsonic(
                .notRunning(causes: [.networkGone(interface: "en0")]),
                networks: [
                    BridgeNetworkChoice(interface: "en0", address: nil),
                    BridgeNetworkChoice(
                        interface: "utun4",
                        address: "100.64.0.3"
                    ),
                ]
            )
        ),
        subsonicPane(
            "settings-subsonic-failed",
            config: .previewSubsonic(network: .interface(interface: "en0")),
            view: .previewSubsonic(
                .notRunning(causes: [
                    .failed(detail: "Permission denied (os error 13)")
                ])
            )
        ),
        subsonicPane(
            "settings-subsonic-port-taken",
            config: .previewSubsonic(network: .interface(interface: "en0")),
            view: .previewSubsonic(.notRunning(causes: [.portTaken]))
        ),
        subsonicPane(
            "settings-subsonic-refused",
            config: .previewSubsonic(
                network: .interface(interface: "en0"),
                username: "",
                port: 4533
            ),
            view: .previewSubsonic(
                .notRunning(causes: [.missingUsername, .missingPassword]),
                passwordSaved: false
            )
        ),
        syncPage("sync-status-synced", .previewSynced),
        syncPage("sync-status-waiting", .previewWaiting),
        syncPage("sync-status-offline", .previewOffline),
        syncPage(
            "sync-status-signed-out",
            .previewSignedOut,
            storage: PreviewData.previewDropboxStorage
        ),
        syncPage("sync-status-failed", .previewFailed),
        syncPage("settings-sync-restore-code", .previewSynced),
    ]

    /// The Automation pane whose MCP server could not start: Status reads
    /// Not running, with the notice and its Details under it, once the pane
    /// has asked for the status.
    private static let automationFailed = ShotScene(
        id: "settings-automation-failed",
        size: CGSize(width: 690, height: 420)
    ) {
        let statusRead = StatusRead()
        return ShotStage(
            AutomationSettingsContent(
                automation: Automation(
                    setMcpServerConfig: { _, _ in },
                    getMcpServerStatus: {
                        await statusRead.markDone()
                        return .error(
                            error: .bindFailed(
                                detail:
                                    "Address already in use (os error 48)"
                            )
                        )
                    },
                    getMcpToken: { "token" },
                    generateMcpToken: { "replacement-token" },
                    setMcpToken: { _ in }
                ),
                configStore: PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    mcpEnabled: true
                ),
                copyToPasteboard: { _ in }
            )
            .environment(UiStore())
        ) {
            statusRead.done
        }
    }

    /// The Subsonic pane showing `view` over the settings in `config`.
    private static func subsonicPane(
        _ id: String,
        config: BridgeSubsonicConfig,
        view: BridgeSubsonicView
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 520)) {
            ShotStage(
                SubsonicSettingsContent(
                    subsonic: SubsonicServer(
                        setEnabled: { _ in },
                        setPort: { _ in },
                        setUsername: { _ in },
                        setNetwork: { _ in },
                        setPassword: { _ in }
                    ),
                    view: view,
                    config: config,
                    names: NetworkNames(byInterface: [
                        "en0": "Wi-Fi", "utun4": "Tailscale",
                    ]),
                    copy: { _ in },
                    showError: { _ in }
                )
                .environment(UiStore())
            )
        }
    }

    /// The Casting tab with casting on and `receivers` found.
    private static func castingSettings(
        _ id: String,
        receivers: BridgeCastReceiverList
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 600)) {
            let castStore = CastStore()
            castStore.receivers = receivers
            return ShotStage(
                CastingSettingsTab()
                    .environment(Cast.stub())
                    .environment(CastStart.stub())
                    .environment(castStore)
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            castEnabled: true
                        )
                    )
                    .environment(UiStore())
            )
        }
    }

    /// The Transfers pane with uploads paused or running and downloads running.
    private static func transfersSettings(
        _ id: String,
        uploadsPaused: Bool
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 420)) {
            ShotStage(
                TransfersSettingsTab()
                    .environment(PreviewData.configStore())
                    .environment(Downloads.stub())
                    .environment(DownloadStore())
                    .environment(OutboxStore(uploadsPaused: uploadsPaused))
                    .environment(Sync.stub())
                    .environment(UiStore())
            )
        }
    }

    /// The Settings sidebar of a library whose sync is `shown`, which names
    /// the state beside Library when it is amber or red.
    private static func settingsSidebar(
        _ id: String,
        _ shown: BridgeSyncShown
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 190, height: 330)) {
            ShotStage(
                SettingsSidebar(selection: .constant(.appearance))
                    .environment(DownloadStore())
                    .environment(OutboxStore(uploadsPaused: false))
                    .environment(
                        SyncStatusStore(
                            snapshot: BridgeSyncStatusSnapshot(
                                shown: shown,
                                pending: [],
                                setupRunning: false,
                                undoneSetup: nil
                            )
                        )
                    )
            )
        }
    }

    /// The Sync page of a library in `storage`, its status block showing
    /// `shown`.
    private static func syncPage(
        _ id: String,
        _ shown: BridgeSyncShown,
        storage: BridgeStorageLocation = PreviewData.previewStorage
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 640)) {
            let fixture = LibrarySettingsTabPreviewFixture(storage: storage)
            return ShotStage(syncSettingsPagePreview(fixture, shown: shown))
        }
    }

    /// The Library tab over a library whose storage is `storage`, or none,
    /// with its sync as `shown` says, once its name and people have answered.
    private static func librarySettings(
        _ id: String,
        storage: BridgeStorageLocation?,
        shown: BridgeSyncShown = .previewSynced
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 600)) {
            let fixture = LibrarySettingsTabPreviewFixture(storage: storage)
            return ShotStage(
                librarySettingsTabPreview(fixture, shown: shown)
            ) {
                fixture.answered
            }
        }
    }
}

extension LibrarySettingsTabPreviewFixture {
    /// Whether both reads have answered, so the tab shows the name and the
    /// people.
    fileprivate var answered: Bool {
        libraryName.name != nil && membership.members != nil
    }
}

/// Whether the Automation pane has asked for the server's status. Observable,
/// as a scene's readiness is read under observation tracking.
@MainActor
@Observable
private final class StatusRead {
    private(set) var done = false

    func markDone() { done = true }
}

extension BridgeSubsonicConfig {
    /// A Subsonic setting as a shot sets it: on, signed in as mara.
    fileprivate static func previewSubsonic(
        enabled: Bool = true,
        network: BridgeSubsonicNetwork = .thisMachine,
        username: String = "mara",
        port: UInt32 = 4533
    ) -> BridgeSubsonicConfig {
        BridgeSubsonicConfig(
            enabled: enabled,
            port: port,
            username: username,
            network: network
        )
    }
}

extension BridgeSubsonicView {
    fileprivate static func previewSubsonic(
        _ status: BridgeSubsonicStatus,
        networks: [BridgeNetworkChoice] = [
            BridgeNetworkChoice(interface: "en0", address: "192.168.1.20"),
            BridgeNetworkChoice(interface: "utun4", address: "100.64.0.3"),
        ],
        passwordSaved: Bool = true
    ) -> BridgeSubsonicView {
        BridgeSubsonicView(
            status: status,
            networks: networks,
            passwordSaved: passwordSaved
        )
    }
}

extension BridgeServedAddress {
    fileprivate static let previewWiFi = BridgeServedAddress(
        url: "http://192.168.1.20:4533",
        interface: "en0"
    )
    fileprivate static let previewTailscale = BridgeServedAddress(
        url: "http://100.64.0.3:4533",
        interface: "utun4"
    )
}
