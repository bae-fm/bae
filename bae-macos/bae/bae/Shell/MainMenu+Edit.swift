import AppKit

/// The Edit menu: AppKit's standard items and actions.
extension MainMenu {
    static func editMenu() -> NSMenu {
        menu(
            "Edit",
            [
                item("Undo", #selector(UndoActions.undo(_:)), "z"),
                item(
                    "Redo",
                    #selector(UndoActions.redo(_:)),
                    "z",
                    [.command, .shift]
                ),
                .separator(),
                item("Cut", #selector(NSText.cut(_:)), "x"),
                item("Copy", #selector(NSText.copy(_:)), "c"),
                item("Paste", #selector(NSText.paste(_:)), "v"),
                item(
                    "Paste and Match Style",
                    #selector(NSTextView.pasteAsPlainText(_:)),
                    "v",
                    [.command, .option, .shift]
                ),
                item("Delete", #selector(NSText.delete(_:))),
                item("Select All", #selector(NSResponder.selectAll(_:)), "a"),
                .separator(),
                submenu("Find", findItems()),
                submenu("Spelling and Grammar", spellingItems()),
                submenu("Substitutions", substitutionItems()),
                submenu("Transformations", transformationItems()),
                submenu("Speech", speechItems()),
            ]
        )
    }

    private static func findItems() -> [NSMenuItem] {
        let find = #selector(NSTextView.performFindPanelAction(_:))
        return [
            item("Find...", find, "f", tag: .showFindInterface),
            item(
                "Find and Replace...",
                find,
                "f",
                [.command, .option],
                tag: .showReplaceInterface
            ),
            item("Find Next", find, "g", tag: .nextMatch),
            item(
                "Find Previous",
                find,
                "g",
                [.command, .shift],
                tag: .previousMatch
            ),
            item("Use Selection for Find", find, "e", tag: .setSearchString),
            item(
                "Jump to Selection",
                #selector(NSTextView.centerSelectionInVisibleArea(_:)),
                "j"
            ),
        ]
    }

    private static func spellingItems() -> [NSMenuItem] {
        [
            item(
                "Show Spelling and Grammar",
                #selector(NSText.showGuessPanel(_:)),
                ":"
            ),
            item(
                "Check Document Now",
                #selector(NSText.checkSpelling(_:)),
                ";"
            ),
            .separator(),
            item(
                "Check Spelling While Typing",
                #selector(NSTextView.toggleContinuousSpellChecking(_:))
            ),
            item(
                "Check Grammar With Spelling",
                #selector(NSTextView.toggleGrammarChecking(_:))
            ),
            item(
                "Correct Spelling Automatically",
                #selector(
                    NSTextView.toggleAutomaticSpellingCorrection(_:)
                )
            ),
        ]
    }

    private static func substitutionItems() -> [NSMenuItem] {
        [
            item(
                "Show Substitutions",
                #selector(NSTextView.orderFrontSubstitutionsPanel(_:))
            ),
            .separator(),
            item(
                "Smart Copy/Paste",
                #selector(NSTextView.toggleSmartInsertDelete(_:))
            ),
            item(
                "Smart Quotes",
                #selector(NSTextView.toggleAutomaticQuoteSubstitution(_:))
            ),
            item(
                "Smart Dashes",
                #selector(NSTextView.toggleAutomaticDashSubstitution(_:))
            ),
            item(
                "Smart Links",
                #selector(NSTextView.toggleAutomaticLinkDetection(_:))
            ),
            item(
                "Data Detectors",
                #selector(NSTextView.toggleAutomaticDataDetection(_:))
            ),
            item(
                "Text Replacement",
                #selector(NSTextView.toggleAutomaticTextReplacement(_:))
            ),
        ]
    }

    private static func transformationItems() -> [NSMenuItem] {
        [
            item(
                "Make Upper Case",
                #selector(NSResponder.uppercaseWord(_:))
            ),
            item(
                "Make Lower Case",
                #selector(NSResponder.lowercaseWord(_:))
            ),
            item("Capitalize", #selector(NSResponder.capitalizeWord(_:))),
        ]
    }

    private static func speechItems() -> [NSMenuItem] {
        [
            item(
                "Start Speaking",
                #selector(NSTextView.startSpeaking(_:))
            ),
            item("Stop Speaking", #selector(NSTextView.stopSpeaking(_:))),
        ]
    }
}

/// The standard Undo and Redo actions, which AppKit's windows and text
/// views take from the Edit menu.
@objc
private protocol UndoActions {
    func undo(_ sender: Any?)
    func redo(_ sender: Any?)
}
