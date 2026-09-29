import AppKit
import Testing

@testable import bae

@MainActor
@Suite("Save format popup")
struct SaveFormatPopupTests {
    private let sameNamedChoices = [
        SaveFormatChoice(
            title: "FLAC",
            extensionName: "flac",
            presetId: "preset-a"
        ),
        SaveFormatChoice(
            title: "FLAC",
            extensionName: "flac",
            presetId: "preset-b"
        ),
    ]

    @Test("presets that share a name each keep their own item")
    func sameNamedPresetsKeepTheirItems() {
        let popup = SaveFormatPopup.make(
            choices: sameNamedChoices,
            selectedPresetId: "preset-b",
            frame: .zero
        )

        #expect(popup.numberOfItems == 2)
        #expect(SaveFormatPopup.selectedPresetId(of: popup) == "preset-b")
    }

    @Test("the selected item names its own preset")
    func selectedItemNamesItsPreset() {
        let popup = SaveFormatPopup.make(
            choices: sameNamedChoices,
            selectedPresetId: "preset-b",
            frame: .zero
        )

        popup.selectItem(at: 0)

        #expect(SaveFormatPopup.selectedPresetId(of: popup) == "preset-a")
    }
}
