import BaeKit
import SwiftUI

/// Zone 2 of the mapping pane: every file in the folder exactly once, with the
/// job the scan proposed for it, what that role makes of it, and the control
/// that changes it.
///
/// A directory core decided to collapse appears as one group row standing in
/// for its files — the roles of fourteen scans are one fact, not fourteen.
struct ImportRolesTable: View {
    let rows: [ImportRoleRow]
    /// What each track sheet may be bound to, by the sheet's file id. Core
    /// probes to decide, so the table is handed the answer: a sheet with no
    /// entry yet shows no picker.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    let actions: ImportRoleActions

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: coreString("ui.import.roles.title"))
            VStack(spacing: 0) {
                headerRow
                ForEach(Array(rows.enumerated()), id: \.element.id) {
                    index,
                    row in
                    body(of: row)
                        .padding(.horizontal, 14)
                        .frame(minHeight: 36)
                        .background(
                            index.isMultiple(of: 2)
                                ? Color.clear : .white.opacity(0.02)
                        )
                }
            }
            .formGroupCard()
        }
    }

    @ViewBuilder
    private func body(of row: ImportRoleRow) -> some View {
        switch row {
        case .file(let file):
            ImportRoleRowView(
                file: file,
                bindingOptions: bindingOptions[file.file.name],
                actions: actions,
            )
        case .directory(let directory):
            ImportRoleGroupRow(directory: directory)
        }
    }

    private var headerRow: some View {
        HStack(spacing: 10) {
            FormEyebrow(text: Text("File"))
                .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(
                text: Text(verbatim: coreString("ui.import.roles.column.role"))
            )
            .frame(width: ImportRoleColumns.role, alignment: .leading)
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.roles.column.becomes")
                )
            )
            .frame(width: ImportRoleColumns.becomes, alignment: .leading)
            Spacer()
                .frame(width: ImportRoleColumns.control)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.13)).frame(height: 1)
        }
    }
}

/// Column widths shared by the roles table's header and its rows, so the two
/// never disagree.
enum ImportRoleColumns {
    static let role: CGFloat = 120
    static let becomes: CGFloat = 130
    static let control: CGFloat = 190
}

#if DEBUG
    // MARK: - Previews

    private func inertRoleActions() -> ImportRoleActions {
        ImportRoleActions(
            setRole: { _, _ in },
            bindSheet: { _, _ in },
            openDocument: { _ in },
            openImage: { _ in },
        )
    }

    #Preview("Roles — CUE+FLAC with a collapsed directory") {
        ImportRolesTable(
            rows: ImportRoleRow.rows(of: PreviewData.bridgeCandidateFiles),
            bindingOptions: PreviewData.sheetBindingOptions,
            actions: inertRoleActions(),
        )
        .padding(20)
        .frame(width: 900)
        .importPreviewEnvironment()
    }

    #Preview("Roles — a sheet that describes nothing") {
        ImportRolesTable(
            rows: [
                .file(PreviewData.unboundTrackSheet),
                .file(PreviewData.refusedTrackSheet),
            ],
            bindingOptions: PreviewData.sheetBindingOptions,
            actions: inertRoleActions(),
        )
        .padding(20)
        .frame(width: 900)
        .importPreviewEnvironment()
    }
#endif
