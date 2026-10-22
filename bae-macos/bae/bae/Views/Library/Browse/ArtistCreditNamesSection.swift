import BaeKit
import SwiftUI

/// The artist's other printed names on its page: each one renamed in place
/// or removed, and a field to add another. Credits printing a name follow it
/// when it is renamed, and print the artist's own name once it is removed.
struct ArtistCreditNamesSection: View {
    let artistId: String
    let names: [BridgeArtistCreditName]
    @Environment(ArtistEditor.self)
    private var artistEditor
    @Environment(UiStore.self)
    private var uiStore
    @State
    private var newName = ""

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text("Also credited as")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            ForEach(names, id: \.credit.key) { name in
                ArtistCreditNameRow(
                    name: name,
                    onRename: { rename(name, to: $0) },
                    onRemove: { remove(name) }
                )
            }
            HStack(spacing: ThemeSpace.related) {
                TextField("Another name", text: $newName)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(add)
                Button("Add", action: add)
                    .disabled(trimmedNewName.isEmpty)
            }
            .frame(maxWidth: 320, alignment: .leading)
        }
    }

    private var trimmedNewName: String {
        newName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func rename(_ name: BridgeArtistCreditName, to renamed: String) {
        Task {
            do {
                try await artistEditor.renameCreditName(
                    name.artistId,
                    name.credit.key,
                    renamed
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func remove(_ name: BridgeArtistCreditName) {
        Task {
            do {
                try await artistEditor.removeCreditName(
                    name.artistId,
                    name.credit.key
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func add() {
        let name = trimmedNewName
        guard !name.isEmpty else { return }
        newName = ""
        Task {
            do {
                try await artistEditor.addCreditName(artistId, name)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}

/// One of the artist's other printed names: edited in place, renamed when the
/// edit is submitted; removed with its button.
private struct ArtistCreditNameRow: View {
    let name: BridgeArtistCreditName
    let onRename: (String) -> Void
    let onRemove: () -> Void
    @State
    private var text: String

    init(
        name: BridgeArtistCreditName,
        onRename: @escaping (String) -> Void,
        onRemove: @escaping () -> Void
    ) {
        self.name = name
        self.onRename = onRename
        self.onRemove = onRemove
        _text = State(initialValue: name.credit.name)
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            TextField(text: $text) { Text(verbatim: name.credit.name) }
                .textFieldStyle(.plain)
                .onSubmit(submit)
            Button(action: onRemove) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(Text("Remove"))
        }
        .frame(maxWidth: 320, alignment: .leading)
        .onChange(of: name.credit.name) { _, renamed in text = renamed }
    }

    private func submit() {
        let renamed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !renamed.isEmpty, renamed != name.credit.name else {
            text = name.credit.name
            return
        }
        onRename(renamed)
    }
}
