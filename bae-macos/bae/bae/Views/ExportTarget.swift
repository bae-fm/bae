import AppKit
import BaeKit
import Foundation

struct ExportFormatChoice {
    let title: String
    let extensionName: String?
    let selection: BridgeExportSelection

    @MainActor
    static func releaseChoices(
        presets: [BridgeExportPreset]
    ) -> [ExportFormatChoice] {
        var choices = [
            ExportFormatChoice(
                title: String(localized: "Original"),
                extensionName: nil,
                selection: .original
            )
        ]
        choices.append(
            contentsOf:
                presets
                .filter(\.appliesToRelease)
                .map {
                    ExportFormatChoice(
                        title: $0.name,
                        extensionName: $0.extension,
                        selection: .preset(presetId: $0.id)
                    )
                }
        )
        return choices
    }
}

struct ReleaseExportTarget {
    let targetDir: String
    let selection: BridgeExportSelection
}

/// Resolves the destination directory for a release export from the
/// export-location setting: a fixed folder returns straight away; "ask each
/// time" opens one `NSOpenPanel`. Returns `nil` when the user picks no folder,
/// so the caller enqueues nothing. Shared by every export-trigger call site.
enum ExportTarget {
    @MainActor
    static func resolveRelease(
        _ location: BridgeExportLocation,
        choices: [ExportFormatChoice],
        defaultSelection: BridgeExportSelection
    ) -> ReleaseExportTarget? {
        precondition(
            !choices.isEmpty,
            "release export choices must include Original"
        )
        guard
            let selectedIndex = selectedFormatIndex(
                choices: choices,
                defaultSelection: defaultSelection
            )
        else {
            showDefaultFormatUnavailableAlert()
            return nil
        }

        switch location {
        case .fixed(let dir):
            guard
                let selection = resolveReleaseSelection(
                    choices: choices,
                    selectedIndex: selectedIndex
                )
            else {
                return nil
            }
            return ReleaseExportTarget(targetDir: dir, selection: selection)
        case .askEachTime:
            let panel = NSOpenPanel()
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.canCreateDirectories = true
            panel.prompt = String(localized: "Export Here")
            let popup = makeFormatPopup(
                choices: choices,
                selectedIndex: selectedIndex
            )
            panel.accessoryView = formatAccessoryView(popup: popup)
            guard panel.runModal() == .OK, let url = panel.url else {
                return nil
            }
            return ReleaseExportTarget(
                targetDir: url.path(percentEncoded: false),
                selection: choices[popup.indexOfSelectedItem].selection
            )
        }
    }

    @MainActor
    private static func resolveReleaseSelection(
        choices: [ExportFormatChoice],
        selectedIndex: Int
    ) -> BridgeExportSelection? {
        let alert = NSAlert()
        alert.messageText = String(localized: "Export")
        alert.addButton(withTitle: String(localized: "Export"))
        alert.addButton(withTitle: String(localized: "Cancel"))
        let popup = makeFormatPopup(
            choices: choices,
            selectedIndex: selectedIndex
        )
        alert.accessoryView = formatAccessoryView(popup: popup)
        guard alert.runModal() == .alertFirstButtonReturn else {
            return nil
        }
        return choices[popup.indexOfSelectedItem].selection
    }

    @MainActor
    private static func selectedFormatIndex(
        choices: [ExportFormatChoice],
        defaultSelection: BridgeExportSelection
    ) -> Int? {
        choices.firstIndex { $0.selection == defaultSelection }
    }

    @MainActor
    private static func showDefaultFormatUnavailableAlert() {
        let alert = NSAlert()
        alert.messageText = String(localized: "Export Failed")
        alert.informativeText = String(localized: "Default format")
        alert.addButton(withTitle: String(localized: "OK"))
        alert.runModal()
    }

    @MainActor
    private static func makeFormatPopup(
        choices: [ExportFormatChoice],
        selectedIndex: Int
    ) -> NSPopUpButton {
        let popup = NSPopUpButton(
            frame: NSRect(x: 54, y: 5, width: 190, height: 24),
            pullsDown: false
        )
        popup.addItems(withTitles: choices.map(\.title))
        popup.selectItem(at: selectedIndex)
        return popup
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
