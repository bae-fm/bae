import AppKit

/// AppKit view for the loudness-measurement bar shown during an import.
///
/// Updated directly from `DesktopUiEvents`' Combine signal, bypassing SwiftUI
/// observation entirely — same pattern as `SeekBarNSView`, so the
/// high-frequency sub-track ticks never re-render the confirm pane tree. The
/// label is the localized `ui.import.loudness_progress` line ("Measuring
/// loudness — N/M"); the bar is the overall scan `fraction`.
class ImportLoudnessProgressNSView: NSView {
    private let label: NSTextField
    private let bar: ProgressTrackNSView

    override init(frame: NSRect) {
        label = NSTextField(labelWithString: "")
        label.font = .systemFont(ofSize: 11)
        label.textColor = .secondaryLabelColor
        label.translatesAutoresizingMaskIntoConstraints = false

        bar = ProgressTrackNSView()
        bar.progress = 0
        bar.translatesAutoresizingMaskIntoConstraints = false

        super.init(frame: frame)

        let stack = NSStackView(views: [label, bar])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 4
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            bar.widthAnchor.constraint(equalToConstant: 200),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    // MARK: - Direct updates (called from DesktopUiEvents, not SwiftUI)

    func setProgress(tracksDone: UInt32, tracksTotal: UInt32, fraction: Double)
    {
        label.stringValue = String(
            format: NSLocalizedString(
                "ui.import.loudness_progress",
                tableName: "Core",
                bundle: .main,
                comment: ""
            ),
            Int(tracksDone),
            Int(tracksTotal)
        )
        bar.progress = fraction
    }
}
