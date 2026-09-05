import BaeKit
import SwiftUI

/// The typed-search form, docked under the result area on every state: which
/// kind of query (General / Catalog # / Barcode), its fields with autocomplete
/// seeded from the folder's scanned text, and Search.
///
/// Every configured provider answers, so the form offers no source selection —
/// a provider that was never asked says so on its own line in the run above.
///
/// The form is the candidate's: what is typed is stored with it, so clicking
/// away and back finds it as it was left. Text is the field's own while it
/// has the keyboard and is committed when the field is left, the tab changes,
/// or Search is pressed — the way the album fields commit.
struct ImportSearchFormView: View {
    /// The form as the candidate stores it. The fields start from it and
    /// follow it while nothing is being typed.
    let form: CandidateSearchState
    /// The form as the person left it, to store with the candidate.
    let onCommit: (CandidateSearchState) -> Void
    let signals: Signals?
    /// A request for the form's first field to take the keyboard — Artist,
    /// the catalog number, or the barcode, whichever the search-by picker
    /// shows. The pane sends one when the result area has nothing to pick
    /// from and when "Search instead" is chosen; each request is a new value.
    let focusRequest: Int
    /// Search with the form as it stands.
    let onSearch: (CandidateSearchState) -> Void

    /// What the fields hold right now. Seeded from `form`, and replaced by a
    /// new `form` only while no field has the keyboard, so a value landing
    /// from core never overwrites what is being typed.
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

    /// Shared suggestion pool for Artist and Album. OCR often runs adjacent
    /// lines of cover text together, so an artist name on the spine may just
    /// as well match the Album field.
    private var generalSuggestions: [String] {
        signals?.text.freeText ?? []
    }

    private var catalogSuggestions: [String] {
        signals?.text.catalogValues ?? []
    }

    /// True while core is still producing suggestions. Drives the small
    /// spinner inside each autocomplete field so users know the list is still
    /// filling in.
    private var isScanning: Bool {
        signals?.text.isScanning ?? false
    }

    private var signalFailure: BridgeLookupFailure? {
        signals?.text.failure
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                FindOnlineCapsLabel("Manual")
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
                    signalFailure.badgeLine,
                    systemImage: "exclamationmark.triangle.fill"
                )
                .font(.system(size: 11.5))
                .foregroundStyle(.orange)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
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
                // The same contract as the autocomplete fields: a pending
                // request is served when the field shows, a new one when it
                // arrives.
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
