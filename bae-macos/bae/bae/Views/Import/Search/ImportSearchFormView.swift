import BaeKit
import SwiftUI

/// The typed-search form: the query kind, its fields with suggestions from the
/// folder's scanned text, and Search.
///
/// What is typed is stored with the candidate, committed when a field is left,
/// the query kind changes, or Search is pressed.
struct ImportSearchFormView: View {
    /// The form as the candidate stores it; the fields follow it while nothing
    /// is being typed.
    let form: CandidateSearchState
    /// The form as the person left it, to store with the candidate.
    let onCommit: (CandidateSearchState) -> Void
    let signals: Signals?
    /// A request for the first field shown to take the keyboard; each request
    /// is a new value.
    let focusRequest: Int
    /// Search with the form as it stands.
    let onSearch: (CandidateSearchState) -> Void

    /// What the fields hold now, replaced by a new `form` only while nothing
    /// is being typed.
    @State
    private var draft = CandidateSearchState()
    @FocusState
    private var barcodeHasFocus: Bool
    @State
    private var isEditing = false

    private var activeTab: SearchTab { draft.activeTab }

    private var isSearchDisabled: Bool {
        switch draft.activeTab {
        case .general:
            draft.searchArtist.isEmpty && draft.searchAlbum.isEmpty
        case .catalogNumber:
            draft.searchCatalog.isEmpty
        case .barcode:
            draft.searchBarcode.isEmpty
        }
    }

    private func submitSearch() {
        guard !isSearchDisabled else { return }
        commit()
        onSearch(draft)
    }

    /// Store the form as it stands, when it differs from what is stored.
    private func commit() {
        if draft != form {
            onCommit(draft)
        }
    }

    private func text(_ field: WritableKeyPath<CandidateSearchState, String>)
        -> Binding<String>
    {
        Binding(
            get: { draft[keyPath: field] },
            set: { draft[keyPath: field] = $0 }
        )
    }

    /// A field's text: typing into it marks the form as being edited, so a
    /// value landing from core waits until the field is left.
    private func editing(_ field: WritableKeyPath<CandidateSearchState, String>)
        -> Binding<String>
    {
        Binding(
            get: { draft[keyPath: field] },
            set: {
                isEditing = true
                draft[keyPath: field] = $0
            }
        )
    }

    private func editingEnded() {
        isEditing = false
        commit()
    }

    /// Suggestions shared by Artist and Album, since text recognition often
    /// runs adjacent cover lines together.
    private var generalSuggestions: [String] {
        signals?.text.freeText ?? []
    }

    private var catalogSuggestions: [String] {
        signals?.text.catalogValues ?? []
    }

    /// Whether core is still producing suggestions, which shows a spinner in
    /// each field.
    private var isScanning: Bool {
        signals?.text.isScanning ?? false
    }

    private var signalFailure: BridgeInternalFailure? {
        signals?.text.failure
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            HStack(spacing: ThemeSpace.related) {
                Picker(
                    "Search by",
                    selection: Binding(
                        get: { draft.activeTab },
                        set: { tab in
                            draft.activeTab = tab
                            commit()
                        }
                    )
                ) {
                    Text("General").tag(SearchTab.general)
                    Text("Catalog #").tag(SearchTab.catalogNumber)
                    Text("Barcode").tag(SearchTab.barcode)
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .controlSize(.small)
                .fixedSize()

                fields

                Button("Search", action: submitSearch)
                    .controlSize(.small)
                    .disabled(isSearchDisabled)
            }
            if let signalFailure {
                Label(
                    signalFailure.detail,
                    systemImage: "exclamationmark.triangle.fill"
                )
                .themeText(.body)
                .foregroundStyle(Theme.warning)
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .padding(.top, ThemeSpace.line)
        .padding(.bottom, ThemeSpace.related)
        .animation(nil, value: activeTab)
        .onChange(of: form, initial: true) { _, stored in
            if !isEditing {
                draft = stored
            }
        }
        .onChange(of: barcodeHasFocus) { _, focused in
            isEditing = focused
            if !focused {
                commit()
            }
        }
        // Clicking another candidate does not end the field edit first, so
        // what was typed goes with the candidate as the pane leaves it.
        .onDisappear(perform: commit)
    }

    @ViewBuilder
    private var fields: some View {
        switch activeTab {
        case .general:
            AutocompleteTextField(
                text: editing(\.searchArtist),
                placeholder: String(localized: "Artist"),
                suggestions: generalSuggestions,
                isLoading: isScanning,
                focusRequest: focusRequest,
                onSubmit: submitSearch,
                onEditingEnded: editingEnded,
            )
            AutocompleteTextField(
                text: editing(\.searchAlbum),
                placeholder: String(localized: "Album"),
                suggestions: generalSuggestions,
                isLoading: isScanning,
                onSubmit: submitSearch,
                onEditingEnded: editingEnded,
            )
        case .catalogNumber:
            AutocompleteTextField(
                text: editing(\.searchCatalog),
                placeholder: String(localized: "e.g. WPCR-80001"),
                suggestions: catalogSuggestions,
                isLoading: isScanning,
                focusRequest: focusRequest,
                onSubmit: submitSearch,
                onEditingEnded: editingEnded,
            )
        case .barcode:
            TextField("e.g. 4943674251780", text: text(\.searchBarcode))
                .textFieldStyle(.roundedBorder)
                .controlSize(.small)
                .focused($barcodeHasFocus)
                .onSubmit(submitSearch)
                // Like the autocomplete fields: serve a pending request when
                // the field shows, and each new one as it arrives.
                .onChange(of: focusRequest, initial: true) { _, request in
                    if request != 0 {
                        barcodeHasFocus = true
                    }
                }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// The form commits to the candidate; a preview holds the stored form.
    private struct ImportSearchFormPreview: View {
        @State
        var form: CandidateSearchState

        var body: some View {
            ImportSearchFormView(
                form: form,
                onCommit: { form = $0 },
                signals: PreviewData.settledSignals,
                focusRequest: 0,
                onSearch: { _ in },
            )
        }
    }

    #Preview("General search") {
        ImportSearchFormPreview(
            form: CandidateSearchState(
                searchArtist: "Artist Name",
                searchAlbum: "Album Title"
            )
        )
        .frame(width: 660)
        .windowBackground()
    }

    #Preview("Catalog search") {
        ImportSearchFormPreview(
            form: CandidateSearchState(
                searchCatalog: "WPCR-80001",
                activeTab: .catalogNumber
            )
        )
        .frame(width: 660)
        .windowBackground()
    }
#endif
