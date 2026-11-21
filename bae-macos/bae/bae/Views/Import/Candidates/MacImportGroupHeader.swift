import BaeKit
import SwiftUI

/// A folder group's header: its chevron and name, folding the group open or
/// shut on a click. Its members follow as rows of their own.
struct MacImportGroupHeader: View {
    let group: BridgeTriageGroup
    let expanded: Bool
    let onSetExpanded: (Bool) -> Void
    /// Read every release below the header's folder as one.
    let onCombine: () -> Void

    var body: some View {
        Button {
            onSetExpanded(!expanded)
        } label: {
            HStack(spacing: ImportListHierarchyLayout.headerSpacing) {
                Image(systemName: expanded ? "chevron.down" : "chevron.forward")
                    .themeIcon(ImportListHierarchyLayout.headerChevron)
                    .foregroundStyle(Theme.textTertiary)
                    .frame(width: ImportListHierarchyLayout.headerChevron.size)
                Text(group.name)
                    .themeText(.strong)
                    .foregroundStyle(Theme.macTextSecondary)
                    .cutLine(.fileName)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
            .padding(ImportCandidateTableLayout.headerPadding)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        // Opaque, so a header floating over the rows below hides them.
        .background(Theme.surface)
        .macContextMenu {
            // A header that only names a shared path component offers
            // nothing.
            if group.combinable {
                MacMenuItem("Combine as one release", action: onCombine)
            }
        }
    }
}
