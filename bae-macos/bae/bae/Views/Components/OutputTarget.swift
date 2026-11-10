import AppKit
import BaeKit
import Foundation

/// One preset the user can pick in a save flow: its display name, the file
/// extension its codec produces (as core gives it), and its id. Built from the
/// configured presets, every one of which a track and a release save offer.
struct SaveFormatChoice {
    let title: String
    let extensionName: String
    let presetId: String

    static func choices(presets: [BridgeSavePreset]) -> [SaveFormatChoice] {
        presets.map {
            SaveFormatChoice(
                title: $0.name,
                extensionName: $0.codec.fileExtension,
                presetId: $0.id
            )
        }
    }
}

/// A resolved release-save destination: the chosen folder plus the preset to
/// render with.
struct ReleaseSaveTarget {
    let targetDir: URL
    let presetId: String
}

/// Destination pickers for release-level output. Export chooses a folder only
/// (verbatim, no format); save chooses a folder plus a preset. Both seed and
/// write back `lastOutputFolder` (per-device UI memory, not synced config).
/// Returns `nil` when the user cancels, so the caller enqueues nothing.
enum OutputTarget {
    /// UserDefaults key for the last folder a release output was written to.
    /// Per-device UI convenience, not synced config — it only seeds the picker.
    private static let lastOutputFolderKey = "lastOutputFolder"

    /// Verbatim export: a plain folder dialog, no format anywhere. Returns the
    /// chosen directory.
    @MainActor
    static func resolveExportDir() -> URL? {
        let panel = makeFolderPanel()
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        UserDefaults.standard.set(
            url.path(percentEncoded: false),
            forKey: lastOutputFolderKey
        )
        return url
    }

    /// Release save: a folder dialog with a format-picker accessory, starting
    /// at the format the last save used. The dialog does not block the app, so
    /// Edit Formats… can open Settings in front of it. Returns the chosen
    /// folder plus preset id.
    @MainActor
    static func resolveReleaseSave(
        configStore: ConfigStore,
        openFormats: @escaping @MainActor () -> Void
    ) async -> ReleaseSaveTarget? {
        let panel = makeFolderPanel()
        let picker = SaveFormatPicker(
            configStore: configStore,
            selectedPresetId: configStore.config.lastSavePreset,
            frame: NSRect(x: 54, y: 5, width: 190, height: 24),
            openFormats: openFormats,
            onChange: { _ in }
        )
        defer { picker.stop() }
        panel.accessoryView = formatAccessoryView(popup: picker.popup)
        let response = await withCheckedContinuation { continuation in
            panel.begin { continuation.resume(returning: $0) }
        }
        guard response == .OK, let url = panel.url else {
            return nil
        }
        UserDefaults.standard.set(
            url.path(percentEncoded: false),
            forKey: lastOutputFolderKey
        )
        return ReleaseSaveTarget(
            targetDir: url,
            presetId: picker.selectedPresetId
        )
    }

    @MainActor
    private static func makeFolderPanel() -> NSOpenPanel {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Export here")
        if let last = UserDefaults.standard.string(forKey: lastOutputFolderKey),
            !last.isEmpty
        {
            panel.directoryURL = URL(fileURLWithPath: last)
        }
        return panel
    }

    @MainActor
    private static func formatAccessoryView(popup: NSPopUpButton) -> NSView {
        let accessoryContainer = NSView(
            frame: NSRect(x: 0, y: 0, width: 250, height: 34)
        )
        let label = NSTextField(labelWithString: String(localized: "Format"))
        label.frame = NSRect(x: 0, y: 7, width: 50, height: 20)
        accessoryContainer.addSubview(label)
        accessoryContainer.addSubview(popup)
        return accessoryContainer
    }
}
