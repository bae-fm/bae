import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac controls the app's forms and rows are built from: buttons,
    /// switches, radios, rows, sections and boxes, and the small parts
    /// beside them.
    static let controlsComponentScenes: [ShotScene] =
        buttonScenes + switchScenes + radioScenes + rowScenes
        + sectionAndBoxScenes + nestedScenes + smallPartScenes
        + menuComponentScenes
        + errorLineScenes + formHeadingScenes + placeholderLineScenes
        + factsGridScenes + searchFieldScenes + progressRingScenes
        + moreMenuStateScenes

    /// `MacButton`: the sizes, the kinds, off, and the icon button.
    private static let buttonScenes: [ShotScene] = [
        component("mac.component.button.sizes", height: 60) {
            HStack(spacing: 8) {
                MacButton("Small", kind: .primary) {}
                    .controlSize(.small)
                MacButton("Regular", kind: .primary) {}
                MacButton("Large", kind: .primary) {}
                    .controlSize(.large)
            }
        },
        component("mac.component.button.primary", height: 50) {
            MacButton("Set up", kind: .primary) {}
        },
        component("mac.component.button.secondary", height: 50) {
            MacButton("Set up") {}
        },
        component("mac.component.button.borderless", height: 50) {
            MacButton("Set up", kind: .borderless) {}
        },
        component("mac.component.button.off", height: 50) {
            MacButton("Set up", kind: .primary) {}
                .disabled(true)
        },
        component("mac.component.button.icon", height: 50) {
            HStack(spacing: 8) {
                MacButton(kind: .borderless, action: {}) {
                    Image(systemName: "doc.on.doc")
                }
                .accessibilityLabel("Copy")
                MacButton(kind: .borderless, action: {}) {
                    Image(systemName: "checkmark")
                }
                .accessibilityLabel("Copied")
            }
        },
    ]

    /// `MacSwitch`, on and off, in the form's switch look.
    private static let switchScenes: [ShotScene] = [
        component("mac.component.switch.on", height: 50) {
            MacSwitch("Restore on launch", isOn: .constant(true))
                .settingsRowControls()
        },
        component("mac.component.switch.off", height: 50) {
            MacSwitch("Restore on launch", isOn: .constant(false))
                .settingsRowControls()
        },
    ]

    /// `MacRadio`, checked and not, and with no note.
    private static let radioScenes: [ShotScene] = [
        radio("mac.component.radio.checked", note: "Your Apple Account."),
        radio(
            "mac.component.radio.unchecked",
            note: "Your Apple Account.",
            checked: false
        ),
        radio("mac.component.radio.no-note"),
    ]

    private static func radio(
        _ id: String,
        note: String? = nil,
        checked: Bool = true
    ) -> ShotScene {
        component(id, width: 596, height: 100) {
            ShotBox {
                MacRadio(
                    title: Text("iCloud"),
                    note: note.map { Text($0) },
                    checked: checked
                )
            }
        }
    }

    /// `MacRow` in a box: its label, a value, a toned value, controls, and
    /// the row under a switch.
    private static let rowScenes: [ShotScene] = [
        component("mac.component.row.label", width: 596, height: 70) {
            ShotBox {
                MacRow("Sync") {}
            }
        },
        component("mac.component.row.value", width: 596, height: 70) {
            ShotBox {
                MacRow("Sync", value: "Synced")
            }
        },
        component("mac.component.row.tone", width: 596, height: 70) {
            ShotBox {
                MacRow("Sync") {
                    ToneDotValue(text: "Synced", tone: .success)
                }
            }
        },
        component("mac.component.row.controls", width: 596, height: 70) {
            ShotBox {
                MacRow("Sync") {
                    MacButton("Set up") {}
                }
            }
        },
        pauseRows("mac.component.row.nested", off: false),
        pauseRows("mac.component.row.nested-off", off: true),
    ]

    /// The Pause row and Resume automatically under it, which is off while
    /// the pause is.
    private static func pauseRows(_ id: String, off: Bool) -> ShotScene {
        component(id, width: 596, height: 120) {
            ShotBox {
                MacRow("Pause between sides and discs") {
                    MacSwitch(
                        "Pause between sides and discs",
                        isOn: .constant(!off)
                    )
                }
                MacNested {
                    MacRow("Resume automatically") {
                        MacPopUpButton(
                            "Resume automatically",
                            selection: .constant(0)
                        ) {
                            MacPopUpOption(verbatim: "Never", value: 0)
                        }
                        .fixedSize()
                    }
                }
                .disabled(off)
            }
        }
    }

    /// `MacSection` over its box, and `MacBox` holding rows.
    private static let sectionAndBoxScenes: [ShotScene] = [
        component("mac.component.section.default", height: 110) {
            ShotForm {
                MacSection("People") {
                    Text("The section’s content")
                }
            }
        },
        component("mac.component.box.rows", width: 596, height: 160) {
            ShotBox {
                MacRow("This Mac") {}
                MacRow("Phone") {}
                MacRow("Laptop") {}
            }
        },
    ]

    /// `MacNested`: the Discogs key's field and the notice about it, under
    /// the switch they belong to.
    private static let nestedScenes: [ShotScene] = [
        component(
            "mac.component.nested.field-and-notice",
            width: 596,
            height: 220
        ) {
            ShotBox {
                SettingToggleRow(
                    title: "Search Discogs",
                    isOn: .constant(true)
                )
                MacNested {
                    DiscogsSettingsContent(
                        draft: .constant(""),
                        status: .rejected,
                        isEnabled: true,
                        isValidating: false,
                        wasRejected: false,
                        saveError: nil,
                        readError: nil,
                        onSave: {},
                        onRecheck: {}
                    )
                }
            }
        }
    ]

    /// The icon button, the hairline, the empty-pane message and the status
    /// glyph.
    private static let smallPartScenes: [ShotScene] = [
        component("mac.component.icon-button.resting", height: 60) {
            MacIconButton(symbol: "shuffle", label: "Shuffle") {}
        },
        component("mac.component.icon-button.active", height: 60) {
            MacIconButton(symbol: "repeat", label: "Repeat", active: true) {}
        },
        component("mac.component.hairline.default", height: 60) {
            MacHairline()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        },
        component("mac.component.content-unavailable.default", height: 200) {
            MacContentUnavailable(title: "Composers")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        },
        component("mac.component.status-glyph.macos", height: 60) {
            HStack(spacing: 16) {
                StatusGlyph(.info)
                StatusGlyph(.success)
                StatusGlyph(.warning)
                StatusGlyph(.danger)
            }
            .font(.system(size: 17))
        },
    ]

    /// `MacMenuItem` plain, checked and highlighted, and an open `MacMenu`,
    /// drawn in place.
    private static let menuComponentScenes: [ShotScene] = [
        component("mac.component.menu-item.plain", height: 100) {
            MacMenuInPlace {
                MacMenuItem("Only me") {}
            }
        },
        component("mac.component.menu-item.checked", height: 100) {
            MacMenuInPlace {
                MacMenuItem("Only me", checked: true) {}
            }
        },
        component("mac.component.menu-item.highlighted", height: 100) {
            MacMenuInPlace(
                {
                    MacMenuItem("Only me") {}
                },
                highlighted: 0
            )
        },
        component("mac.component.menu.open", height: 140) {
            MacMenuInPlace(
                {
                    MacMenuItem("Off") {}
                    MacMenuItem("Only me", checked: true) {}
                    MacMenuItem("Everyone") {}
                },
                highlighted: 1
            )
        },
    ]

    /// `MacErrorLine`: one line, and a message that wraps.
    private static let errorLineScenes: [ShotScene] = [
        component("mac.component.error-line.error", height: 50) {
            MacErrorLine("The pattern needs at least one value.")
        },
        component("mac.component.error-line.long-error", width: 596, height: 70)
        {
            MacErrorLine(
                "A name cannot contain “/” or “:”, because macOS uses them to separate folders. Take them out of the typed text."
            )
            .frame(maxWidth: .infinity, alignment: .leading)
        },
    ]

    /// `MacFormHeading`, a ruled heading over a part of a form.
    private static let formHeadingScenes: [ShotScene] = [
        component("mac.component.form-heading.heading", width: 596, height: 50)
        {
            MacFormHeading(title: "Release")
                .frame(width: 480)
        }
    ]

    /// `MacPlaceholderLine`: a line, a shorter one, a table cell's and a
    /// picture's.
    private static let placeholderLineScenes: [ShotScene] = [
        component("mac.component.placeholder-line.line", height: 40) {
            MacPlaceholderLine(width: 140, height: 12)
        },
        component("mac.component.placeholder-line.short-line", height: 40) {
            MacPlaceholderLine(width: 90, height: 10)
        },
        component("mac.component.placeholder-line.table-cell", height: 40) {
            MacPlaceholderLine(width: 180, height: 14)
        },
        component("mac.component.placeholder-line.picture", height: 70) {
            MacPlaceholderLine(
                width: 44,
                height: 44,
                radius: ThemeRadius.artwork
            )
        },
    ]

    /// `MacFactsGrid`: a file's facts, fewer facts, a value that wraps, and
    /// an album's credits in the wide look.
    private static let factsGridScenes: [ShotScene] = [
        component(
            "mac.component.facts-grid.file-facts",
            width: 596,
            height: 110
        ) {
            MacFactsGrid {
                MacFact("Folder") {
                    Text(verbatim: "Mara Quill/Low Tide Hours")
                }
                MacFact("Kind") { Text(verbatim: "Audio") }
                MacFact("Format") { Text(verbatim: "FLAC 16/44.1") }
                MacFact("Size") { Text(verbatim: "34.1 MB").monospacedDigit() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
        component("mac.component.facts-grid.some-facts", width: 596, height: 70)
        {
            MacFactsGrid {
                MacFact("Kind") { Text(verbatim: "Rip log") }
                MacFact("Size") { Text(verbatim: "6 KB").monospacedDigit() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
        component(
            "mac.component.facts-grid.long-value",
            width: 596,
            height: 100
        ) {
            MacFactsGrid {
                MacFact("Folder") {
                    Text(
                        verbatim:
                            "The Slow Orchards/Songs for the Slow Orchards, Recorded Live at the Harbor Hall/Disc 2"
                    )
                }
                MacFact("Size") { Text(verbatim: "98.4 MB").monospacedDigit() }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
        component("mac.component.facts-grid.wide", width: 596, height: 90) {
            MacFactsGrid(wide: true) {
                MacFact("Producer") { Text(verbatim: "Ines Varga") }
                MacFact("Engineer") { Text(verbatim: "Tomas Brell") }
                MacFact("Arranger") {
                    Text(verbatim: "Odile Fen (tracks 2, 5)")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        },
    ]

    /// `MacSearchField` as a search, a filter and an inline filter.
    private static let searchFieldScenes: [ShotScene] = [
        searchField("mac.component.search-field.search", kind: .search),
        searchField(
            "mac.component.search-field.search-typed",
            kind: .search,
            text: "Low Tide"
        ),
        searchField(
            "mac.component.search-field.search-long",
            kind: .search,
            text: "Low Tide Hours by Mara Quill and the harbor recordings"
        ),
        searchField(
            "mac.component.search-field.search-focused",
            kind: .search,
            text: "Low Tide",
            focusing: true
        ),
        searchField(
            "mac.component.search-field.filter",
            kind: .filter,
            prompt: "Filter"
        ),
        searchField(
            "mac.component.search-field.filter-typed",
            kind: .filter,
            prompt: "Filter",
            text: "Low Tide"
        ),
        searchField(
            "mac.component.search-field.filter-off",
            kind: .filter,
            prompt: "Filter",
            off: true
        ),
        searchField(
            "mac.component.search-field.inline",
            kind: .inline,
            prompt: "Filter"
        ),
        searchField(
            "mac.component.search-field.inline-typed",
            kind: .inline,
            prompt: "Filter",
            text: "Low Tide"
        ),
    ]

    private static func searchField(
        _ id: String,
        kind: MacSearchFieldKind,
        prompt: LocalizedStringKey = "Search",
        text: String = "",
        focusing: Bool = false,
        off: Bool = false
    ) -> ShotScene {
        component(id, height: 50) {
            ShotSearchField(
                kind: kind,
                prompt: prompt,
                text: text,
                focusing: focusing
            )
            // The title bar's field is 300 points wide; the inline field
            // fills a row that wide in the preview.
            .frame(
                width: kind == .filter ? nil : 300,
                alignment: .leading
            )
            .disabled(off)
        }
    }

    /// `MacProgressRing` in the tones the Storage Manager draws: a file
    /// moving to the cloud and a file pinned to this Mac.
    private static let progressRingScenes: [ShotScene] = [
        progressRing("mac.component.progress-ring.activity", kind: .upload),
        progressRing("mac.component.progress-ring.pinning", kind: .pin),
    ]

    private static func progressRing(
        _ id: String,
        kind: StorageTransferKind
    ) -> ShotScene {
        component(id, height: 40) {
            MacProgressRing(
                color: kind.color,
                source: OutboxStore(
                    uploadsPaused: false,
                    fileShares: .fileShares(holding: ["file": 0.4])
                )
                .fileShare(forFile: "file")
            )
            .frame(width: 16, height: 16)
        }
    }

    /// `MacMoreMenu` off and over a cover, at the end of a row.
    private static let moreMenuStateScenes: [ShotScene] = [
        component("mac.component.more-menu.off", width: 596, height: 60) {
            MacMoreMenu {
                MacMenuItem("Details…") {}
            }
            .disabled(true)
            .frame(maxWidth: .infinity, alignment: .trailing)
        },
        component("mac.component.more-menu.overlay", width: 596, height: 60) {
            MacMoreMenu(overlay: true) {
                MacMenuItem("Details…") {}
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        },
    ]
}

/// A `MacSearchField` holding its own text and focus, for a shot.
private struct ShotSearchField: View {
    let kind: MacSearchFieldKind
    let prompt: LocalizedStringKey
    let focusing: Bool
    @State
    private var text: String
    @FocusState
    private var focused: Bool

    init(
        kind: MacSearchFieldKind,
        prompt: LocalizedStringKey,
        text: String,
        focusing: Bool
    ) {
        self.kind = kind
        self.prompt = prompt
        self.focusing = focusing
        _text = State(initialValue: text)
    }

    var body: some View {
        MacSearchField(
            kind: kind,
            text: $text,
            prompt: prompt,
            focused: $focused
        )
        .onAppear { focused = focusing }
    }
}
