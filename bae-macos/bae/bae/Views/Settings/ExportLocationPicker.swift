import AppKit
import BaeKit
import SwiftUI

/// Release-export destination control, styled as a radio group like a browser's
/// "Downloads" location setting: pick a fixed folder — with a "Location" row
/// that shows the path and a "Change" button — or prompt for a folder on every
/// export. Writes through the `Exports` service; the change round-trips back via
/// a `configChanged` event into `ConfigStore`, so the selection updates
/// reactively without an optimistic write.
///
/// `lastExportFolder` is per-device UI convenience state (a local filesystem
/// path): the folder to restore when the user re-selects "Save to a folder",
/// and the greyed path shown while "Ask each time" is active. The authoritative
/// destination stays the config enum, so it's written only after a fixed
/// selection actually lands.
struct ExportLocationPicker: View {
    let configStore: ConfigStore
    let setLocation: @Sendable (BridgeExportLocation) throws -> Void
    let showError: @MainActor (DisplayError) -> Void

    @AppStorage("lastExportFolder")
    private var lastExportFolder = ""

    var body: some View {
        Group {
            radioRow(
                title: "Save to a folder",
                isSelected: isFixed,
                action: selectFixed
            )

            LabeledContent("Location") {
                HStack(spacing: 8) {
                    Text(locationPath)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Divider()
                        .frame(height: 16)
                    Button("Change", action: chooseFolder)
                }
            }
            .disabled(!isFixed)
            .foregroundStyle(isFixed ? Color.primary : Color.secondary)

            radioRow(
                title: "Ask each time",
                isSelected: !isFixed,
                action: selectAskEachTime
            )
        }
    }

    /// A selectable row with a leading radio indicator; the whole row is the
    /// hit target so a tap anywhere selects the option.
    private func radioRow(
        title: LocalizedStringKey,
        isSelected: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(
                    systemName: isSelected
                        ? "largecircle.fill.circle" : "circle"
                )
                .foregroundStyle(isSelected ? Color.accentColor : .secondary)
                Text(title)
                Spacer()
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var isFixed: Bool {
        if case .fixed = configStore.config.exportLocation { return true }
        return false
    }

    /// The path shown in the Location row: the configured folder when fixed,
    /// otherwise the last-remembered folder, or a placeholder when the user has
    /// never chosen one.
    private var locationPath: String {
        switch configStore.config.exportLocation {
        case .fixed(let dir):
            return dir
        case .askEachTime:
            return lastExportFolder.isEmpty
                ? String(localized: "No folder chosen")
                : lastExportFolder
        }
    }

    /// Select "Save to a folder". Reuse the remembered folder if there is one;
    /// otherwise prompt, and if the user cancels, stay on "Ask each time".
    private func selectFixed() {
        guard !isFixed else { return }
        if lastExportFolder.isEmpty {
            chooseFolder()
        }
        else {
            apply(.fixed(dir: lastExportFolder))
        }
    }

    private func selectAskEachTime() {
        guard isFixed else { return }
        apply(.askEachTime)
    }

    /// Prompt for a folder (directories only). On pick, set a fixed
    /// destination; on cancel, make no change.
    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Choose")
        guard panel.runModal() == .OK, let url = panel.url else { return }
        apply(.fixed(dir: url.path(percentEncoded: false)))
    }

    /// Write the destination through the service, remembering the folder only
    /// after a fixed selection actually lands. The config change round-trips
    /// via `configChanged`; `lastExportFolder` is local UI state.
    private func apply(_ location: BridgeExportLocation) {
        do {
            try setLocation(location)
            if case .fixed(let dir) = location {
                lastExportFolder = dir
            }
        }
        catch let error as BridgeError {
            showError(DisplayError(error))
        }
        catch {
            showError(DisplayError(line: error.localizedDescription))
        }
    }
}
