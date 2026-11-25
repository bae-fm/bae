import BaeKit
import SwiftUI

/// A catalog number extraction found and the run is not looking up: the
/// number, outlined. Pressing it puts the number into the run and every
/// provider looks it up.
struct CatalogCandidateChip: View {
    let value: String
    let onActivate: () -> Void

    var body: some View {
        MacIdentifierChip(
            label: String(localized: "Catalog #"),
            value: value,
            style: .outlined,
            onPress: onActivate
        ) {
            EmptyView()
        }
        .help("Look this catalog number up")
    }
}

/// The button that lists the catalog numbers nothing confirmed, or folds
/// them again.
struct CatalogCandidatesDisclosure: View {
    let title: String
    @Binding
    var isExpanded: Bool

    var body: some View {
        // The button sets what it will do when pressed: list the numbers
        // while they are folded, fold them while they are listed.
        let expandsOnPress = !isExpanded
        Button {
            isExpanded = expandsOnPress
        } label: {
            StatusChip(verbatim: title, tone: .neutral)
        }
        .buttonStyle(.accentPlain)
        .help(
            "Catalog numbers found in the folder that no release confirms"
        )
    }
}
