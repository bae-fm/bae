import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ReleaseFileTable")

/// A release's files in its Storage sheet. The table draws the files in the
/// order core returns them and sends column sorts back to the sheet.
struct MacReleaseFileTable: NSViewRepresentable {
    let files: [BridgeFile]
    @Binding
    var order: BridgeReleaseFileOrder
    @Binding
    var presentationError: DisplayError?

    @Environment(\.surfaceTone)
    private var tone
    @Environment(\.accentChoice)
    private var accent

    private var appearance: HostedAppearance {
        HostedAppearance(tone: tone, accent: accent)
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(
            order: $order,
            presentationError: $presentationError,
            appearance: appearance
        )
    }

    func makeNSView(context: Context) -> NSScrollView {
        let table = context.coordinator.table
        table.style = .inset
        // The sheet supplies the surface behind the rows and their stripes.
        table.backgroundColor = .clear
        table.rowHeight = 28
        table.intercellSpacing = .zero
        table.columnAutoresizingStyle = .firstColumnOnlyAutoresizingStyle
        table.allowsColumnResizing = true
        table.allowsColumnReordering = false
        table.dataSource = context.coordinator
        table.delegate = context.coordinator
        for column in Column.inDisplayOrder {
            table.addTableColumn(column.makeTableColumn())
        }
        return table.themedScrollView()
    }

    func updateNSView(_: NSScrollView, context: Context) {
        context.coordinator.update(
            files: files,
            order: $order,
            presentationError: $presentationError,
            appearance: appearance
        )
    }

    private enum Column: Sendable {
        case sortable(BridgeReleaseFileColumn)
        case audio

        static let inDisplayOrder: [Self] = [
            .sortable(.name), .audio, .sortable(.size), .sortable(.kind),
        ]

        static let byKey = Dictionary(
            uniqueKeysWithValues: inDisplayOrder.map { ($0.key, $0) }
        )

        var key: String {
            switch self {
            case .sortable(.name): "name"
            case .audio: "audio"
            case .sortable(.size): "size"
            case .sortable(.kind): "kind"
            }
        }

        private var title: String {
            switch self {
            case .sortable(.name): String(localized: "Name")
            case .audio: localizedCoreString("core.audio.label")
            case .sortable(.size): String(localized: "Size")
            case .sortable(.kind): String(localized: "Kind")
            }
        }

        var width: CGFloat {
            switch self {
            case .sortable(.name): 200
            case .audio: 120
            case .sortable(.size): 80
            case .sortable(.kind): 130
            }
        }

        @MainActor
        func makeTableColumn() -> NSTableColumn {
            let column = NSTableColumn(
                identifier: NSUserInterfaceItemIdentifier(key)
            )
            column.headerCell.attributedStringValue = NSAttributedString(
                string: title,
                attributes: [
                    .font: ThemeText.detail.nsFont,
                    .foregroundColor: NSColor(Theme.macTextSecondary),
                ]
            )
            column.isEditable = false
            column.minWidth = width
            column.width = width
            if case .sortable = self {
                column.sortDescriptorPrototype = NSSortDescriptor(
                    key: key,
                    ascending: true
                )
            }
            return column
        }

        @MainActor
        @ViewBuilder
        func cell(for file: BridgeFile) -> some View {
            switch self {
            case .sortable(.name):
                Text(file.originalFilename)
                    .cutLine(.fileName)
                    .foregroundStyle(Theme.textPrimary)
            case .audio:
                if let format = file.audioFormat {
                    Text(format.text)
                        .foregroundStyle(Theme.macTextSecondary)
                        .lineLimit(1)
                }
            case .sortable(.size):
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(Theme.macTextSecondary)
            case .sortable(.kind):
                Text(file.contentType)
                    .foregroundStyle(Theme.textTertiary)
                    .lineLimit(1)
            }
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTableViewDataSource,
        NSTableViewDelegate
    {
        let table = NSTableView()
        private var order: Binding<BridgeReleaseFileOrder>
        private var presentationError: Binding<DisplayError?>
        private var appearance: HostedAppearance
        private var files: [BridgeFile] = []

        init(
            order: Binding<BridgeReleaseFileOrder>,
            presentationError: Binding<DisplayError?>,
            appearance: HostedAppearance
        ) {
            self.order = order
            self.presentationError = presentationError
            self.appearance = appearance
        }

        func update(
            files: [BridgeFile],
            order: Binding<BridgeReleaseFileOrder>,
            presentationError: Binding<DisplayError?>,
            appearance: HostedAppearance
        ) {
            self.order = order
            self.presentationError = presentationError
            let shown = order.wrappedValue
            table.showSortIndicator(
                key: Column.sortable(shown.column).key,
                ascending: !shown.descending
            )
            if files != self.files || appearance != self.appearance {
                self.files = files
                self.appearance = appearance
                table.reloadData()
            }
        }

        func numberOfRows(in table: NSTableView) -> Int { files.count }

        func tableView(_: NSTableView, shouldSelectRow _: Int) -> Bool {
            false
        }

        func tableView(_: NSTableView, rowViewForRow row: Int)
            -> NSTableRowView?
        {
            BrowsedTableRowView(
                accent: appearance.accent,
                stripedRow: row
            )
        }

        func tableView(
            _ table: NSTableView,
            viewFor tableColumn: NSTableColumn?,
            row: Int
        ) -> NSView? {
            guard let identifier = tableColumn?.identifier,
                let column = Column.byKey[identifier.rawValue]
            else {
                let key = tableColumn?.identifier.rawValue ?? "none"
                reportBug(
                    operation: "Draw a release file cell",
                    detail: "Unknown release file column \(key)"
                )
                return nil
            }
            let cell = HostingTableCell.dequeue(
                from: table,
                identifier: identifier,
                owner: self
            )
            cell.host(
                column.cell(for: files[row])
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .hostedAppearance(appearance)
            )
            return cell
        }

        func tableView(
            _ table: NSTableView,
            sortDescriptorsDidChange _: [NSSortDescriptor]
        ) {
            guard let sort = table.sortDescriptors.first,
                let key = sort.key,
                case .sortable(let column)? = Column.byKey[key]
            else {
                let key = table.sortDescriptors.first?.key ?? "none"
                reportBug(
                    operation: "Sort release files",
                    detail: "Unknown release file sort column \(key)"
                )
                return
            }
            let selected = BridgeReleaseFileOrder(
                column: column,
                descending: !sort.ascending
            )
            if order.wrappedValue != selected {
                order.wrappedValue = selected
            }
        }

        private func reportBug(operation: String, detail: String) {
            logger.error("\(operation) failed: \(detail)")
            CoreBugReporter().report(operation: operation, detail: detail)
            let error = DisplayError(
                BridgeError.Diagnostic(
                    category: .internal,
                    detail: "\(operation): \(detail)"
                )
            )
            // AppKit can call this during the representable's update.
            Task { @MainActor [presentationError] in
                presentationError.wrappedValue = error
            }
        }
    }
}
