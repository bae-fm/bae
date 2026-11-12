import AppKit
import SwiftUI

/// A pop-up whose menu lists each choice in full and whose closed button
/// shows a shorter title for the chosen one: "Wi-Fi" on the button over a menu
/// of "Wi-Fi · 192.168.1.20". A SwiftUI `Picker` closes on the chosen item's
/// own text, so it cannot.
struct ClosedTitlePopUp<Choice: Hashable>: NSViewRepresentable {
    /// One line of the menu.
    enum Entry {
        /// A choice with its title in the menu and the title the closed
        /// button shows while it is chosen.
        case choice(Choice, title: String, closedTitle: String)
        case separator
    }

    let entries: [Entry]
    @Binding
    var selection: Choice
    let accessibilityLabel: String

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSPopUpButton {
        let popUp = NSPopUpButton(frame: .zero, pullsDown: false)
        popUp.target = context.coordinator
        popUp.action = #selector(Coordinator.chose(_:))
        popUp.setContentHuggingPriority(.required, for: .horizontal)
        // The button shows its own item, not the chosen one of the menu.
        (popUp.cell as? NSPopUpButtonCell)?.usesItemFromMenu = false
        return popUp
    }

    func updateNSView(_ popUp: NSPopUpButton, context: Context) {
        context.coordinator.selection = $selection
        popUp.setAccessibilityLabel(accessibilityLabel)
        popUp.removeAllItems()
        var closedTitle = ""
        context.coordinator.choices = []
        for entry in entries {
            switch entry {
            case .separator:
                popUp.menu?.addItem(.separator())
            case .choice(let choice, let title, let short):
                let item = NSMenuItem(
                    title: title,
                    action: nil,
                    keyEquivalent: ""
                )
                item.tag = context.coordinator.choices.count
                context.coordinator.choices.append(choice)
                let chosen = choice == selection
                item.state = chosen ? .on : .off
                if chosen { closedTitle = short }
                popUp.menu?.addItem(item)
            }
        }
        (popUp.cell as? NSPopUpButtonCell)?.menuItem = NSMenuItem(
            title: closedTitle,
            action: nil,
            keyEquivalent: ""
        )
        popUp.sizeToFit()
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: NSPopUpButton,
        context _: Context
    ) -> CGSize? {
        nsView.intrinsicContentSize
    }

    @MainActor
    final class Coordinator: NSObject {
        var selection: Binding<Choice>?
        var choices: [Choice] = []

        @objc
        func chose(_ sender: NSPopUpButton) {
            guard let tag = sender.selectedItem?.tag,
                choices.indices.contains(tag)
            else { return }
            selection?.wrappedValue = choices[tag]
        }
    }
}
