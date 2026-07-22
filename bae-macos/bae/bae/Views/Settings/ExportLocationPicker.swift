import AppKit
import BaeKit
import SwiftUI

/// Release-export destination control, styled like a browser's download
/// location popup: the remembered folder (when there is one), "Ask each time",
/// and "Other…" which prompts for a folder. Writes through the `Exports`
/// service; the change round-trips back via a `configChanged` event into
/// `ConfigStore`, so the selection updates reactively without an optimistic
/// write — a cancelled prompt simply re-renders the stored choice.
///
/// `lastExportFolder` is per-device UI convenience state (a local filesystem
/// path): while "Ask each time" is active it keeps the previous folder on the
/// menu so re-selecting it needs no prompt. The authoritative destination
/// stays the config enum, so it's written only after a fixed selection
/// actually lands.
struct ExportLocationPicker: View {
    let configStore: ConfigStore
    let setLocation: @Sendable (BridgeExportLocation) throws -> Void
    /// Takes the error, not a rendered line: the sink decides whether there is
    /// anything to show (a cancellation is not).
    let showError: @MainActor (any Error) -> Void

    @AppStorage("lastExportFolder")
    private var lastExportFolder = ""

    /// One popup entry. The folder case carries the full path; the menu shows
    /// its display name.
    private enum Choice: Hashable {
        case folder(String)
        case askEachTime
        case chooseOther
    }

    var body: some View {
        Picker("Location", selection: selectionBinding) {
            if let folder = offeredFolder {
                Text(folderDisplayName(folder)).tag(Choice.folder(folder))
            }
            Text("Ask each time").tag(Choice.askEachTime)
            Divider()
            Text("Other…").tag(Choice.chooseOther)
        }
        .help(offeredFolder ?? "")
    }

    private var selectionBinding: Binding<Choice> {
        Binding(
            get: {
                switch configStore.config.exportLocation {
                case .fixed(let dir): .folder(dir)
                case .askEachTime: .askEachTime
                }
            },
            set: { choice in
                switch choice {
                case .folder(let dir):
                    apply(.fixed(dir: dir))
                case .askEachTime:
                    apply(.askEachTime)
                case .chooseOther:
                    // Defer past the binding write: the panel runs modal, and
                    // the picker re-renders from config either way — the
                    // chosen folder when one lands, the stored choice on
                    // cancel.
                    Task { @MainActor in chooseFolder() }
                }
            }
        )
    }

    /// The folder entry on the menu: the configured folder when fixed,
    /// otherwise the remembered one, or none when the user never chose a
    /// folder.
    private var offeredFolder: String? {
        switch configStore.config.exportLocation {
        case .fixed(let dir):
            return dir
        case .askEachTime:
            return lastExportFolder.isEmpty ? nil : lastExportFolder
        }
    }

    private func folderDisplayName(_ dir: String) -> String {
        URL(fileURLWithPath: dir).lastPathComponent
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
        catch {
            showError(error)
        }
    }
}
