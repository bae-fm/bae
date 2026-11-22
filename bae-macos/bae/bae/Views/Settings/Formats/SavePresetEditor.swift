import BaeKit
import SwiftUI

/// The edit sheet for one export preset; every control writes the whole
/// preset through `update`, and the name through `rename`, so Done only
/// closes it.
struct SavePresetEditor: View {
    let preset: BridgeSavePreset
    /// Every format, this one included, for the name a format change gives.
    let presets: [BridgeSavePreset]
    let update: (BridgeSavePreset) -> Void
    /// Write the preset with a new name; core answers whether another format
    /// has it.
    let rename:
        @MainActor (BridgeSavePreset) async throws -> BridgeSavePresetWrite

    @Environment(\.dismiss)
    private var dismiss
    @Environment(UiStore.self)
    private var uiStore

    /// The name field, committed on submit or focus loss; an empty draft
    /// disables Done instead.
    @State
    private var nameDraft = ""
    @FocusState
    private var nameFocused: Bool
    /// The name core last refused because another format has it, while the
    /// field still holds it.
    @State
    private var refusedName: String?
    /// The write of the last name the person committed, while it runs. A new
    /// commit and the sheet's close cancel it, so a reply to an older name
    /// never marks the name the field holds now.
    @State
    private var renaming: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacBox {
                nameRow
                formatRow
                qualityRow
            }
            MacSheetFooter {
                MacButton("Done", kind: .primary) { dismiss() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!nameValid)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.top, ThemeSpace.group)
        .padding(.bottom, ThemeSpace.edge)
        .frame(width: 480)
        .task(id: preset.name) {
            nameDraft = preset.name
        }
        .onDisappear { renaming?.cancel() }
    }

    private var nameValid: Bool {
        !nameDraft.trimmingCharacters(in: .whitespaces).isEmpty && !nameTaken
    }

    /// Core refused the name in the field: another format has it.
    private var nameTaken: Bool {
        refusedName == nameDraft
    }

    /// The names of the other formats.
    private var otherNames: [String] {
        presets.filter { $0.id != preset.id }.map(\.name)
    }

    /// The format's one quality setting: a bit depth for a lossless codec, a
    /// bitrate from the format's list for a lossy one.
    @ViewBuilder
    private var qualityRow: some View {
        if let bitDepth = preset.codec.bitDepth {
            PresetBitDepthRow(
                preset: preset,
                bitDepth: bitDepth,
                update: update
            )
        }
        else if let bitrate = preset.codec.bitrateKbps,
            let accepted = bridgeSaveFormatBitratesKbps(
                format: preset.codec.format
            )
        {
            PresetBitrateRow(
                preset: preset,
                bitrate: bitrate,
                accepted: accepted,
                update: update
            )
        }
    }

    private var nameRow: some View {
        FieldColumn {
            MacTextField(
                "Name",
                error: nameTaken ? "Another format has this name." : nil
            ) {
                BoxedField(
                    text: $nameDraft,
                    focus: $nameFocused,
                    equals: true,
                    invalid: nameTaken
                )
                .onSubmit(commitName)
                .onChange(of: nameFocused) { _, focused in
                    if !focused {
                        commitName()
                    }
                }
            }
        }
        .settingsRowInset(ThemeSpace.compact)
    }

    /// The formats the Format pop-up offers, in core's order and groups.
    @MacPopUpBuilder<BridgeSaveFormat>
    static var formatOptions: [MacPopUpEntry<BridgeSaveFormat>] {
        let groups = bridgeSaveFormatGroups()
        for group in groups.indices {
            if group > 0 {
                MacMenuItem.divider
            }
            for format in groups[group] {
                MacPopUpOption(verbatim: format.label, value: format)
            }
        }
    }

    private var formatRow: some View {
        MacRow("Format") {
            formatPicker
        }
    }

    private var formatPicker: some View {
        MacPopUpButton(
            "Format",
            selection: Binding(
                get: { preset.codec.format },
                set: { format in
                    var changed = preset
                    changed.codec = preset.codec.switched(to: format)
                    // A name that is still a format's prefill follows the
                    // format; a name the user typed stays put.
                    do {
                        changed.name = try bridgeSaveNameAfterFormatChange(
                            name: preset.name,
                            from: preset.codec.format,
                            to: format,
                            taken: otherNames,
                            zeroDigit: Outputs.zeroDigit
                        )
                    }
                    catch {
                        uiStore.showError(error)
                        return
                    }
                    update(changed)
                }
            )
        ) {
            Self.formatOptions
        }
    }

    private func commitName() {
        guard nameValid, nameDraft != preset.name else { return }
        var changed = preset
        changed.name = nameDraft
        renaming?.cancel()
        renaming = Task {
            do {
                let written = try await rename(changed)
                try Task.checkCancellation()
                switch written {
                case .saved: refusedName = nil
                case .nameTaken: refusedName = changed.name
                }
            }
            catch is CancellationError {
                return
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
