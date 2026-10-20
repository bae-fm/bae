import BaeKit
import SwiftUI

/// The frame a Found or Skipped row draws in: the cover at the list's artwork
/// size — the placeholder when there is none, so every row's text lines up —
/// then the row's text, then its trailing column. Its height must not change
/// on selection, which would shift every row below.
struct ImportListRowLayout<Meta: View, Trailing: View>: View {
    let coverContent: ImageContent?
    @ViewBuilder
    let meta: () -> Meta
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        HStack(alignment: .center, spacing: ThemeSpace.related) {
            ImageView(
                content: coverContent,
                pointSize: TriageRowView.coverPointSize
            )
            .frame(
                width: TriageRowView.coverPointSize,
                height: TriageRowView.coverPointSize
            )
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            meta()
            Spacer(minLength: ThemeSpace.inline)
            trailing()
        }
        .padding(.vertical, ThemeSpace.compact)
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
    }
}

extension ImportListRowLayout where Trailing == EmptyView {
    init(
        coverContent: ImageContent?,
        @ViewBuilder meta: @escaping () -> Meta
    ) {
        self.init(coverContent: coverContent, meta: meta) { EmptyView() }
    }
}

/// A row that names its folder, as the main pane's heading does: one with no
/// release read for it yet, with the caller's accessory after the name.
struct ImportFolderNameLine<Accessory: View>: View {
    let folderName: String
    @ViewBuilder
    let accessory: () -> Accessory

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            Image(systemName: "folder")
                .themeIcon(.medium)
                .foregroundStyle(.secondary)
            Text(folderName)
                .themeText(.mono)
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            accessory()
                .layoutPriority(1)
        }
    }
}

extension ImportFolderNameLine where Accessory == EmptyView {
    init(folderName: String) {
        self.init(folderName: folderName) { EmptyView() }
    }
}
