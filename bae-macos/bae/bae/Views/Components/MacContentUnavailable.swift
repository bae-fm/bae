import BaeKit
import SwiftUI

/// The message of a pane with nothing to show: a title, a line under it where
/// there is something to add, and `actions` under that where the person can
/// act on it, centred in the space the pane has.
struct MacContentUnavailable<Actions: View>: View {
    let title: Text
    let description: Text?
    @ViewBuilder
    let actions: () -> Actions

    init(
        title: LocalizedStringKey,
        description: LocalizedStringKey? = nil,
        @ViewBuilder actions: @escaping () -> Actions
    ) {
        self.title = Text(title)
        self.description = description.map { Text($0) }
        self.actions = actions
    }

    /// A title that is already in the person's language, such as one that
    /// names a person.
    init(
        verbatimTitle title: String,
        @ViewBuilder actions: @escaping () -> Actions
    ) {
        self.title = Text(verbatim: title)
        self.description = nil
        self.actions = actions
    }

    var body: some View {
        if let description {
            ContentUnavailableView {
                title.themeText(.contentTitle)
            } description: {
                description.themeText(.body)
                    .foregroundStyle(Theme.textSecondary)
            } actions: {
                actions()
            }
        }
        else {
            ContentUnavailableView {
                title.themeText(.contentTitle)
            } actions: {
                actions()
            }
        }
    }
}

extension MacContentUnavailable where Actions == EmptyView {
    init(title: LocalizedStringKey, description: LocalizedStringKey? = nil) {
        self.init(
            title: title,
            description: description,
            actions: { EmptyView() }
        )
    }
}
