import AppKit
import OSLog
import SwiftUI

private let logger = Logger.bae("StorageTableColumns")

// MARK: - Column layout

/// The table's columns as the person arranges them: shown or hidden from the
/// header's menu, resized and reordered by dragging, and sized to their
/// content by double-clicking a header's divider. Each change is written to
/// the coordinator's `layout`, which the table applies.
extension StorageTableView.Coordinator {
    /// Show `layout` in the table: all of it the first time, and after that
    /// what changed since the layout the table last took or wrote. The first
    /// column's width follows the window, which a width written before it
    /// moved would undo.
    func applyLayout(_ layout: StorageTableLayout) {
        guard let tableView, layout != appliedLayout else { return }
        let previous = appliedLayout
        appliedLayout = layout
        // Moving and resizing columns here posts the notifications a person's
        // change does; none of them is a change to write back.
        applyingLayout = true
        defer { applyingLayout = false }
        for (index, entry) in layout.columns.enumerated() {
            let at = tableView.column(
                withIdentifier: NSUserInterfaceItemIdentifier(
                    entry.column.rawValue
                )
            )
            guard at >= 0 else {
                logger.error(
                    "The storage table has no \(entry.column.rawValue) column"
                )
                continue
            }
            let tableColumn = tableView.tableColumns[at]
            let before = previous?.columns
                .first {
                    $0.column == entry.column
                }
            if before?.hidden != entry.hidden {
                tableColumn.isHidden = entry.hidden
            }
            if before?.width != entry.width {
                tableColumn.width = entry.width
            }
            if at != index {
                tableView.moveColumn(at, toColumn: index)
            }
        }
    }

    /// Write the table's columns as they stand to `layout`, `fitted` at the
    /// width it is about to take.
    private func writeLayout(
        fitted: (column: StorageTableColumn, width: CGFloat)? = nil
    ) {
        guard let tableView, !applyingLayout else { return }
        let columns = tableView.tableColumns.compactMap {
            tableColumn -> StorageTableLayout.Column? in
            guard
                let column = StorageTableColumn(
                    rawValue: tableColumn.identifier.rawValue
                )
            else {
                logger.error(
                    "No storage column for \(tableColumn.identifier.rawValue)"
                )
                return nil
            }
            return StorageTableLayout.Column(
                column: column,
                width: fitted.flatMap { $0.column == column ? $0.width : nil }
                    ?? tableColumn.width,
                hidden: tableColumn.isHidden
            )
        }
        guard let next = StorageTableLayout(holding: columns) else {
            logger.error("The storage table doesn't hold every column once")
            return
        }
        appliedLayout = next
        if layout.wrappedValue != next {
            layout.wrappedValue = next
        }
    }

    func tableViewColumnDidMove(_ notification: Notification) {
        writeLayout()
    }

    func tableViewColumnDidResize(_ notification: Notification) {
        // Only a width the person drags is kept: the first column also
        // follows the window's width, which is no choice of theirs.
        guard let headerView = tableView?.headerView,
            headerView.resizedColumn >= 0
        else {
            return
        }
        writeLayout()
    }

    /// Double-clicking the divider after a column's header sizes the column
    /// to its header and the rows in view, the rows the list has loaded.
    func tableView(
        _ tableView: NSTableView,
        sizeToFitWidthOfColumn index: Int
    ) -> CGFloat {
        let tableColumn = tableView.tableColumns[index]
        guard
            let column = StorageTableColumn(
                rawValue: tableColumn.identifier.rawValue
            )
        else {
            logger.error(
                "No storage column for \(tableColumn.identifier.rawValue)"
            )
            return tableColumn.width
        }
        let width = max(
            column.minWidth,
            fittingWidth(of: column, at: index, in: tableView)
        )
        writeLayout(fitted: (column, width))
        return width
    }

    /// The width `column`, at `index`, needs to show its header, sort
    /// indicator included, and each row in view whole.
    private func fittingWidth(
        of column: StorageTableColumn,
        at index: Int,
        in tableView: NSTableView
    ) -> CGFloat {
        var width: CGFloat = 0
        if let headerView = tableView.headerView {
            let bounds = headerView.headerRect(ofColumn: index)
            let header = tableView.tableColumns[index].headerCell
            width =
                header.cellSize(forBounds: bounds).width
                + header.sortIndicatorRect(forBounds: bounds).width
        }
        let rows = tableView.rows(in: tableView.visibleRect)
        for row in rows.location..<(rows.location + rows.length) {
            let cell = NSHostingView(
                rootView: content(forRow: row, column: column)
            )
            width = max(
                width,
                cell.fittingSize.width + tableView.intercellSpacing.width
            )
        }
        return ceil(width)
    }

    // MARK: Header menu

    func makeColumnsMenu() -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        return menu
    }

    /// One item per column, checked while it shows. The release column's
    /// item stays checked and can't be chosen: it always shows.
    func addColumnItems(to menu: NSMenu) {
        let layout = layout.wrappedValue
        for column in StorageTableColumn.allCases {
            let item = NSMenuItem(
                title: column.title,
                action: #selector(setColumnShown(_:)),
                keyEquivalent: ""
            )
            item.target = self
            item.representedObject = column.rawValue
            item.state = layout.isHidden(column) ? .off : .on
            item.isEnabled = column.hideable
            menu.addItem(item)
        }
    }

    /// Show the item's column if it is hidden, hide it if it shows.
    @objc
    private func setColumnShown(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String,
            let column = StorageTableColumn(rawValue: raw)
        else {
            logger.error("Column menu item carried no storage column")
            return
        }
        layout.wrappedValue = layout.wrappedValue.setting(
            column,
            hidden: sender.state == .on
        )
    }
}
