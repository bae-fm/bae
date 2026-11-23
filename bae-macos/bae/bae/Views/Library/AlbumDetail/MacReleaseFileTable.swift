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

    @Environment(\.surfaceTone)
    private var tone
    @Environment(\.accentChoice)
    private var accent

    private var appearance: HostedAppearance {
        HostedAppearance(tone: tone, accent: accent)
    }

    func makeCoordinator() -> Coordinator {
        Coordinator($order, appearance: appearance)
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
        for column in Column.allCases {
            table.addTableColumn(column.makeTableColumn())
        }
        return table.themedScrollView()
    }

    func updateNSView(_: NSScrollView, context: Context) {
        context.coordinator.update(
            files: files,
            order: $order,
            appearance: appearance
        )
    }

    private enum Column: String, CaseIterable {
        case name, audio, size, kind

        private var identifier: NSUserInterfaceItemIdentifier {
            NSUserInterfaceItemIdentifier(rawValue)
        }

        private var title: String {
            switch self {
            case .name: String(localized: "Name")
            case .audio: localizedCoreString("core.audio.label")
            case .size: String(localized: "Size")
            case .kind: String(localized: "Kind")
            }
        }

        var sortColumn: BridgeReleaseFileColumn? {
            switch self {
            case .name: .name
            case .audio: nil
            case .size: .size
            case .kind: .kind
            }
        }

        init(sortColumn: BridgeReleaseFileColumn) {
            switch sortColumn {
            case .name: self = .name
            case .size: self = .size
            case .kind: self = .kind
            }
        }

        var width: CGFloat {
            switch self {
            case .name: 200
            case .audio: 120
            case .size: 80
            case .kind: 130
            }
        }

        @MainActor
        func makeTableColumn() -> NSTableColumn {
            let column = NSTableColumn(identifier: identifier)
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
            if sortColumn != nil {
                column.sortDescriptorPrototype = NSSortDescriptor(
                    key: rawValue,
                    ascending: true
                )
            }
            return column
        }

        @MainActor
        @ViewBuilder
        func cell(for file: BridgeFile) -> some View {
            switch self {
            case .name:
                Text(file.originalFilename)
                    .cutLine(.fileName)
                    .foregroundStyle(Theme.textPrimary)
            case .audio:
                if let format = file.audioFormat {
                    Text(format.text)
                        .foregroundStyle(Theme.macTextSecondary)
                        .lineLimit(1)
                }
            case .size:
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(Theme.macTextSecondary)
            case .kind:
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
        private var appearance: HostedAppearance
        private var files: [BridgeFile] = []

        init(
            _ order: Binding<BridgeReleaseFileOrder>,
            appearance: HostedAppearance
        ) {
            self.order = order
            self.appearance = appearance
        }

        func update(
            files: [BridgeFile],
            order: Binding<BridgeReleaseFileOrder>,
            appearance: HostedAppearance
        ) {
            self.order = order
            let shown = order.wrappedValue
            let descriptor = NSSortDescriptor(
                key: Column(sortColumn: shown.column).rawValue,
                ascending: !shown.descending
            )
            if table.sortDescriptors.first != descriptor {
                table.sortDescriptors = [descriptor]
            }
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
                let column = Column(rawValue: identifier.rawValue)
            else {
                logger.error(
                    "Unknown release file column: \(tableColumn?.identifier.rawValue ?? "nil")"
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
                let column = Column(rawValue: key)?.sortColumn
            else {
                logger.error("Unknown release file table sort")
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
    }
}
