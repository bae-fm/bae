import BaeKit
import SwiftUI

/// One file of the folder in the roles table: what it is called, the job in
/// force for it, what that job makes of it, and the control that changes it.
struct ImportRoleRowView: View {
    let file: BridgeCandidateFile
    /// What this file may be bound to, when it is a track sheet. `nil` until
    /// core has been asked; empty means there is nothing to offer.
    let bindingOptions: [BridgeSheetBindingOption]?
    let actions: ImportRoleActions

    var body: some View {
        HStack(spacing: 10) {
            nameCell
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(coreString(bridgeFileRoleKey(role: file.role)))
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(width: ImportRoleColumns.role, alignment: .leading)
            Text(bridgeFileBecomesText(file.becomes))
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: ImportRoleColumns.becomes, alignment: .leading)
            control
                .frame(width: ImportRoleColumns.control, alignment: .leading)
        }
    }

    /// The file's own name in mono, its directory prefix dimmed ahead of it,
    /// and the size. Opening it is the affordance where there is something to
    /// open — a document or a track sheet in the viewer, an image in the
    /// lightbox.
    @ViewBuilder
    private var nameCell: some View {
        if let open = openAction {
            Button(action: open) {
                nameLine.contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        else {
            nameLine
        }
    }

    private var nameLine: some View {
        HStack(spacing: 8) {
            (Text(file.file.dirPrefix ?? "").foregroundStyle(.tertiary)
                + Text(file.file.fileName))
                .font(.system(size: 12, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
            Text(file.file.sizeText)
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private var openAction: (() -> Void)? {
        if file.role.isImage {
            return { actions.openImage(file.file.localPath) }
        }
        if file.role.isDocument || file.role.isTrackSheet {
            return { actions.openDocument(file.file) }
        }
        return nil
    }

    /// A track sheet carries its binding; an audio file carries the roles it
    /// can be put in. Nothing else is anybody's decision to make, so nothing
    /// else has a control.
    @ViewBuilder
    private var control: some View {
        if file.role.isTrackSheet {
            ImportSheetBindingMenu(
                sheet: file,
                options: bindingOptions,
                onBind: { actions.bindSheet(file.file.name, $0) },
            )
        }
        else if !file.alternatives.isEmpty {
            ImportRoleChoiceControl(
                alternatives: file.alternatives,
                inForce: file.roleChoice,
                onPick: { actions.setRole(file.file.name, $0) },
            )
        }
    }
}

/// A directory whose files all do the same job, as the one row core decided it
/// should be: the prefix, what it holds, and the total size.
struct ImportRoleGroupRow: View {
    let directory: BridgeCollapsedDirectory

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                Text(directory.dirPrefix)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(
                    verbatim: "\u{2014} "
                        + bridgeFileRowKindText(
                            directory.kind,
                            count: directory.count
                        )
                )
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                Text(
                    Int64(directory.totalSize)
                        .formatted(.byteCount(style: .file))
                )
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Spacer()
                .frame(
                    width: ImportRoleColumns.role + ImportRoleColumns.becomes
                        + ImportRoleColumns.control + 20
                )
        }
    }
}
