import BaeKit
import Testing

/// What a menu's builder collects, which is all the menus' sites hand over.
@MainActor
@Suite("Mac menu builder")
struct MacMenuTests {
    @Test
    func builderKeepsOrderAcrossSectionsLoopsAndConditions() {
        let on = true
        let off = false
        let entries = menu {
            MacMenuItem(verbatim: "Play") {}
            MacMenuDivider()
            MacMenuSection(verbatim: "Queue") {
                for name in ["Next", "Last"] {
                    MacMenuItem(verbatim: name) {}
                }
            }
            if on {
                MacMenuSubmenu(verbatim: "More") {
                    MacMenuItem(verbatim: "Inside") {}
                }
            }
            if off {
                MacMenuItem(verbatim: "Never") {}
            }
        }
        #expect(
            entries.map(describe)
                == ["Play", "-", "#Queue", "Next", "Last", ">More[Inside]"]
        )
    }

    @Test
    func itemsKeepTheirMarkAndDisabledState() {
        let entries = menu {
            MacMenuItem(verbatim: "A", checked: true) {}
            MacMenuItem(verbatim: "B", disabled: true) {}
        }
        guard case .item(let a) = entries[0], case .item(let b) = entries[1]
        else {
            Issue.record("two items")
            return
        }
        #expect(a.isChecked)
        #expect(!a.isDisabled)
        #expect(b.isDisabled)
    }

    @Test
    func popUpBuilderListsOptionsAndDividersInOrder() {
        let entries = popUp {
            MacPopUpOption(verbatim: "FLAC", value: 1)
            MacMenuDivider()
            for n in 2...3 {
                MacPopUpOption(verbatim: "n\(n)", value: n)
            }
        }
        let described = entries.map { entry -> String in
            switch entry {
            case .divider: "-"
            case .option(let option): "\(option.title)=\(option.value)"
            }
        }
        #expect(described == ["FLAC=1", "-", "n2=2", "n3=3"])
    }

    private func menu(
        @MacMenuBuilder _ content: () -> [MacMenuEntry]
    ) -> [MacMenuEntry] {
        content()
    }

    private func popUp(
        @MacPopUpBuilder<Int> _ content: () -> [MacPopUpEntry<Int>]
    ) -> [MacPopUpEntry<Int>] {
        content()
    }

    private func describe(_ entry: MacMenuEntry) -> String {
        switch entry {
        case .item(let item): item.title
        case .divider: "-"
        case .header(let title): "#\(title)"
        case .submenu(let title, let inside, _):
            ">\(title)[\(inside.map(describe).joined(separator: ","))]"
        }
    }
}
