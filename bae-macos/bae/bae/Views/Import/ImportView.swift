import AppKit
import BaeKit
import SwiftUI

// MARK: - Shared helpers

enum CandidateSortOrder: String, CaseIterable {
    case nameAZ
    case nameZA
    case dateAddedNewest
    case dateAddedOldest
}

struct ImportCheckboxToggle: View {
    let title: LocalizedStringKey
    @Binding
    var isOn: Bool

    init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        self.title = title
        _isOn = isOn
    }

    var body: some View {
        Toggle(title, isOn: $isOn)
            .toggleStyle(.checkbox)
            .font(.callout)
    }
}

/// Shared commit tail for the import tabs. Shapes the candidate's raw editor
/// form into the wire edit — writing bae-core's `.invalid` reason onto the
/// candidate and bailing if it doesn't validate (the Import button is disabled
/// while invalid, so that path is defensive) — then runs the tab-specific
/// `start`, writing any thrown error onto the candidate.
@MainActor
func commitImport(
    store: ImportStore,
    key: String,
    rawEdit: BridgeRawReleaseEdit,
    start: (BridgeReleaseUserEdit) throws -> Void
) {
    let userEdit: BridgeReleaseUserEdit
    switch shapeReleaseEdit(raw: rawEdit) {
    case .valid(let edit):
        userEdit = edit
    case .invalid(let reason):
        store.mutateCandidate(forKey: key) {
            $0.error = reason.localizedMessage
        }
        return
    }
    do {
        try start(userEdit)
    }
    catch {
        store.mutateCandidate(forKey: key) {
            $0.error = error.localizedDescription
        }
    }
}

// MARK: - ImportView

struct ImportView: View {
    var body: some View {
        VStack(spacing: 0) {
            Divider()
            FolderImportTab()
        }
    }
}
