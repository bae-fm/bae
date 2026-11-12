import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components the settings panes are built from: the accent
    /// swatches, the cast row, copy text, the disclosure, the field rows, the
    /// file name field, the import destination and the info tip.
    static let settingsComponentScenes: [ShotScene] =
        [
            component("mac.component.accent-picker.resting", height: 60) {
                MacAccentPicker(selected: .pink, onSelect: { _ in })
            },
            component("mac.component.close-button.close", height: 60) {
                MacCloseButton(onClose: {})
            },
            component("mac.component.error-details.causes", height: 90) {
                MacErrorDetails(
                    detail: "The sign-in settings file is missing.",
                    excerpt:
                        "The sign-in settings file is missing.\nOAuthConfig: missing client id"
                )
                .frame(width: 380)
            },
            component("mac.component.info-tip.after-a-row-name", height: 90) {
                ShotBox {
                    SettingToggleRow(
                        title: "Search Discogs",
                        isOn: .constant(false)
                    ) {
                        DiscogsInfoTip()
                    }
                }
            },
        ] + checkboxScenes + castRowScenes + copyTextScenes + disclosureScenes
        + downloadRowScenes + fieldRowScenes + fileNameFieldScenes
        + importDestinationScenes

    /// `MacCheckbox`, off and on.
    private static let checkboxScenes: [ShotScene] = [
        component("mac.component.checkbox.off", height: 40) {
            Toggle("Group by artist", isOn: .constant(false))
                .toggleStyle(.accentCheckbox)
                .themeText(.body)
        },
        component("mac.component.checkbox.on", height: 40) {
            Toggle("Group by artist", isOn: .constant(true))
                .toggleStyle(.accentCheckbox)
                .themeText(.body)
        },
    ]

    private static func receiver(
        _ name: String,
        _ kind: BridgeRendererKind
    ) -> BridgeCastReceiver {
        BridgeCastReceiver(
            id: name,
            name: name,
            kind: kind,
            model: nil,
            address: "192.168.1.41"
        )
    }

    /// `CastReceiverRow` in the cast popover.
    private static let castRowScenes: [ShotScene] = [
        component("mac.component.cast-row.receivers", height: 270) {
            ShotBox {
                CastReceiverRow(
                    receiver: receiver("Living Room", .cast),
                    role: .casting(onDisconnect: {}),
                    place: .picker
                )
                CastReceiverRow(
                    receiver: receiver("Kitchen", .airPlay),
                    role: .found(onCast: {}),
                    place: .picker
                )
                CastReceiverRow(
                    receiver: receiver("Den receiver", .dlna),
                    role: .found(onCast: {}),
                    place: .picker
                )
            }
        },
        component("mac.component.cast-row.long-name", height: 140) {
            ShotBox {
                CastReceiverRow(
                    receiver: receiver(
                        "Denon AVR-X3800H Home Theater Receiver (Basement)",
                        .dlna
                    ),
                    role: .found(onCast: {}),
                    place: .picker
                )
            }
        },
    ]

    /// `MacCopyText` in a Subsonic address row.
    private static let copyTextScenes: [ShotScene] = [
        copyText(
            "mac.component.copy-text.one-address",
            [("http://192.168.1.20:4533", nil)],
            hover: false
        ),
        copyText(
            "mac.component.copy-text.hovered",
            [("http://192.168.1.20:4533", nil)],
            hover: true
        ),
        copyText(
            "mac.component.copy-text.several-addresses",
            [
                ("http://192.168.1.20:4533", "(Wi-Fi)"),
                ("http://100.64.0.3:4533", "(Tailscale)"),
            ],
            hover: false
        ),
    ]

    private static func copyText(
        _ id: String,
        _ addresses: [(String, String?)],
        hover: Bool
    ) -> ShotScene {
        component(id, height: 130) {
            ShotBox {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text("Available on")
                    VStack(alignment: .leading, spacing: ThemeSpace.line) {
                        if addresses.count == 1 {
                            HStack(spacing: ThemeSpace.inline) {
                                Text("Apps connect at")
                                MacCopyText(
                                    text: addresses[0].0,
                                    after: addresses[0].1,
                                    hover: hover,
                                    copy: { _ in }
                                )
                            }
                        }
                        else {
                            Text("Apps connect at:")
                            ForEach(addresses, id: \.0) { address in
                                HStack(
                                    alignment: .firstTextBaseline,
                                    spacing: ThemeSpace.inline
                                ) {
                                    Text(verbatim: "•")
                                    MacCopyText(
                                        text: address.0,
                                        after: address.1,
                                        hover: hover,
                                        copy: { _ in }
                                    )
                                }
                            }
                        }
                    }
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                }
            }
        }
    }

    /// `FormDisclosure` in a grouped form.
    private static let disclosureScenes: [ShotScene] = [
        component("mac.component.disclosure.closed", height: 90) {
            Form {
                FormDisclosure(isExpanded: .constant(false)) {
                } label: {
                    Text("Advanced")
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
        },
        component("mac.component.disclosure.open", height: 170) {
            Form {
                FormDisclosure(isExpanded: .constant(true)) {
                    LabeledContent("Bit depth") {
                        MacPopUpButton("Bit depth", selection: .constant(16)) {
                            MacPopUpOption(verbatim: "16-bit", value: 16)
                        }
                        .fixedSize()
                    }
                    Toggle("Keep the pregap", isOn: .constant(false))
                } label: {
                    Text("Advanced")
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
        },
    ]

    /// `DownloadRow` after a pin or a make-local failed.
    private static let downloadRowScenes: [ShotScene] = [
        downloadRow("mac.component.download-row.pin", kind: .pin),
        downloadRow("mac.component.download-row.make-local", kind: .makeLocal),
    ]

    private static func downloadRow(
        _ id: String,
        kind: BridgeDownloadKind
    ) -> ShotScene {
        component(id, height: 130) {
            let op = BridgeDownloadOp(
                releaseId: "rel-glass-harbor",
                kind: kind,
                release: BridgeQueuedRelease(
                    title: "Glass Harbor",
                    fileCount: 9,
                    totalSize: 356_000_000
                ),
                createdAt: 0,
                state: .failed(error: "The network connection was lost.")
            )
            return DownloadRow(
                op: op,
                transfer: .pin(
                    PreviewData.downloadStore()
                        .downloadProgress(forRelease: op.releaseId)
                ),
                onRetry: {},
                onCancel: {}
            )
        }
    }

    /// `MacFieldRow`, a boxed field with the label on its text line.
    private static let fieldRowScenes: [ShotScene] = [
        component("mac.component.field-row.field", height: 90) {
            ShotBox {
                MacFieldRow("Name") { nameField }
            }
        },
        component("mac.component.field-row.control-after", height: 90) {
            ShotBox {
                MacFieldRow("Name") {
                    nameField
                    Button("Set up") {}
                }
            }
        },
    ]

    private static var nameField: some View {
        TextField("", text: .constant("Home music"))
            .labelsHidden()
            .boxedTextField()
            .frame(width: 160)
    }

    /// `SaveNameField`: the file name pattern with its preview, the folder
    /// pattern, and a pattern the name can't be made from.
    private static let fileNameFieldScenes: [ShotScene] = [
        component("mac.component.file-name-field.file-name", height: 200) {
            Form {
                SaveNameField(
                    title: "File name",
                    pieces: PreviewData.exportFilenamePieces,
                    error: nil,
                    previewSuffix: ".flac",
                    setPieces: { _ in },
                    fail: { _ in }
                )
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
        },
        component("mac.component.file-name-field.typed-text", height: 200) {
            Form {
                SaveNameField(
                    title: "Folder name",
                    pieces: PreviewData.exportFolderPieces,
                    error: nil,
                    setPieces: { _ in },
                    fail: { _ in }
                )
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
        },
        component("mac.component.file-name-field.error", height: 200) {
            Form {
                SaveNameField<BridgeSaveFolderPiece>(
                    title: "Folder name",
                    pieces: [
                        .value(value: .artist), .text(text: " / "),
                        .value(value: .album),
                    ],
                    error: .forbiddenCharacter(character: "/"),
                    setPieces: { _ in },
                    fail: { _ in }
                )
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
        },
    ]

    /// `MacImportDestination`, the pop-up of where an import goes.
    private static let importDestinationScenes: [ShotScene] = [
        component("mac.component.import-destination.this-mac", height: 60) {
            MacImportDestination(destination: .constant(.thisDevice))
        },
        component("mac.component.import-destination.cloud", height: 60) {
            MacImportDestination(destination: .constant(.cloud))
        },
        component("mac.component.import-destination.off", height: 60) {
            MacImportDestination(destination: .constant(.cloud))
                .disabled(true)
        },
    ]
}
