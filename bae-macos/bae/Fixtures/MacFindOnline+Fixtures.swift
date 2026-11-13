import BaeKit
import Foundation
import SwiftUI

@testable import bae

// MARK: - Shot staging

extension MacFindOnline {
    /// A pane with inert form bindings and callbacks, so a shot states
    /// only its situation.
    @MainActor
    static func preview(
        state: ImportSearchState,
        searchArtist: String = "",
        searchAlbum: String = "",
        openSection: BridgeFindOnlineSection = .automatic,
        onRetryFailed: @escaping () -> Void = {},
        showsDiscogsBar: Bool = false,
    ) -> MacFindOnline {
        MacFindOnline(
            state: state,
            onBack: {},
            onCancelIdentification: {},
            form: CandidateSearchState(
                searchArtist: searchArtist,
                searchAlbum: searchAlbum
            ),
            onCommitForm: { _ in },
            onSearch: { _ in },
            onRetrySearch: {},
            onOpenSettings: {},
            onToggleLookup: { _ in },
            onToggleCatalogAgreement: { _ in },
            onRetryFailed: onRetryFailed,
            onEditTitleSearch: { _, _ in },
            onSelect: { _ in },
            onKeepOwnDraft: {},
            onLinkSharedAlbum: {},
            openSection: openSection,
            onOpenSection: { _ in },
            showsDiscogsBar: showsDiscogsBar,
            onDismissDiscogsBar: {}
        )
    }
}
