import OSLog
import SwiftUI

private let logger = Logger.bae("EditMetadataSheet")

// MARK: - EditMetadataSheet

/// Editor sheet for user-owned metadata changes, presented from the album /
/// release detail "Edit metadata..." menu.
///
/// Identity is unaffected by edits — this sheet does not surface or modify
/// `release_identities` rows or the `metadata_source` columns.
struct EditMetadataSheet: View {
    /// Receives the shaped wire edit (the sheet shapes the raw form via
    /// bae-core before calling this), and applies it.
    let onSave: (BridgeReleaseUserEdit) async throws -> Void
    /// Re-projects raw form values from the editor's source pointer
    /// (current persisted state for post-commit). Returning `nil` means
    /// the caller has no projection.
    let onReset: (() async throws -> BridgeRawReleaseEdit?)?
    let onCancel: () -> Void

    @State
    private var form: BridgeRawReleaseEdit
    @State
    private var saving = false
    @State
    private var resetting = false
    @State
    private var errorMessage: String?
    @State
    private var saveTask: Task<Void, Never>?
    @State
    private var resetTask: Task<Void, Never>?

    init(
        initialForm: BridgeRawReleaseEdit,
        onSave: @escaping (BridgeReleaseUserEdit) async throws -> Void,
        onReset: (() async throws -> BridgeRawReleaseEdit?)? = nil,
        onCancel: @escaping () -> Void,
    ) {
        self.onSave = onSave
        self.onReset = onReset
        self.onCancel = onCancel
        _form = State(initialValue: initialForm)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView {
                EditMetadataForm(form: $form)
                    .padding(20)
            }
            if let message = validationMessage ?? errorMessage {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message)
                }
                .font(.caption)
                .foregroundStyle(.red)
                .padding(.horizontal)
                .padding(.bottom, 8)
            }
            footer
        }
        .frame(width: 640, height: 640)
        .background(Theme.background)
        .onDisappear {
            saveTask?.cancel()
            resetTask?.cancel()
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            Text("Edit Metadata")
                .font(.headline)
            Spacer()
            Button("Cancel") { onCancel() }
                .keyboardShortcut(.cancelAction)
                .disabled(saving || resetting)
        }
        .padding()
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 12) {
            if let onReset {
                Button("Reset to Source") {
                    triggerReset(onReset: onReset)
                }
                .disabled(saving || resetting)
            }
            Spacer()
            if saving || resetting {
                ProgressView()
                    .controlSize(.small)
                Text(saving ? "Saving..." : "Resetting...")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            else {
                Button("Save") { triggerSave() }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(savableEdit == nil)
            }
        }
        .padding()
    }

    // MARK: - Shaping

    /// The shaped wire edit when the form is savable, else `nil`. bae-core
    /// owns the normalization + validation; the sheet only reads the outcome
    /// to gate Save and to build the commit payload.
    private var savableEdit: BridgeReleaseUserEdit? {
        if case .valid(let edit) = shapeReleaseEdit(raw: form) {
            return edit
        }
        return nil
    }

    /// The reason the form can't be saved, shaped by bae-core from its
    /// `EditValidationError`, or `nil` when the form is savable. Shown below
    /// the form so a disabled Save button always states why.
    private var validationMessage: String? {
        if case .invalid(let message) = shapeReleaseEdit(raw: form) {
            return message
        }
        return nil
    }

    // MARK: - Actions

    private func triggerSave() {
        // Save is disabled unless the form shapes cleanly, so a savable
        // payload exists by construction; reaching here without one means the
        // disabled gate and the shape result disagreed.
        guard let edit = savableEdit else {
            logger.error("Save triggered with an unshapable form")
            return
        }
        saveTask?.cancel()
        errorMessage = nil
        saving = true
        saveTask = Task {
            do {
                try await onSave(edit)
                saving = false
            }
            catch is CancellationError {
                // Sheet dismissed mid-save; teardown owns the next transition.
            }
            catch {
                saving = false
                errorMessage = "Save failed: \(error.localizedDescription)"
            }
        }
    }

    private func triggerReset(
        onReset: @escaping () async throws -> BridgeRawReleaseEdit?
    ) {
        resetTask?.cancel()
        errorMessage = nil
        resetting = true
        resetTask = Task {
            do {
                if let projected = try await onReset() {
                    form = projected
                }
                resetting = false
            }
            catch is CancellationError {
                // Sheet dismissed mid-reset; teardown owns the next transition.
            }
            catch {
                resetting = false
                errorMessage = "Reset failed: \(error.localizedDescription)"
            }
        }
    }
}

// MARK: - Previews

#Preview("Edit Metadata") {
    EditMetadataSheet(
        initialForm: BridgeRawReleaseEdit(
            albumTitle: "Album Title",
            albumArtistText: "Artist Name",
            pressing: BridgeRawPressingEdit(
                year: "2024",
                format: "Vinyl",
                label: "Some Label",
                catalogNumber: "CAT-001",
                country: "US",
                barcode: ""
            ),
            tracks: [
                BridgeRawTrackEdit(
                    id: "t-1",
                    title: "Track One",
                    artistText: "",
                    side: 1,
                    trackNumber: 1
                ),
                BridgeRawTrackEdit(
                    id: "t-2",
                    title: "Track Two",
                    artistText: "",
                    side: 1,
                    trackNumber: 2
                ),
            ]
        ),
        onSave: { _ in },
        onCancel: {},
    )
}
