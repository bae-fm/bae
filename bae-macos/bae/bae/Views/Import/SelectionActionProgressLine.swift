import AppKit
import BaeKit
import SwiftUI

/// How far a bulk action over the selection has got: what it does to how many
/// folders, over a bar of the ones it has finished, set straight from its
/// progress stream once per display frame. One report arrives per folder, so
/// it runs no SwiftUI update.
final class SelectionActionProgressNSView: NSView {
    private let action: BridgeCandidateAction
    private let label = NSTextField(labelWithString: "")
    private let bar: ProgressTrackNSView
    private lazy var following = FrameFollowing<BridgeSelectionActionProgress>(
        view: self
    ) { [weak self] progress in self?.show(progress) }

    var accent: NSColor {
        get { bar.accent }
        set { bar.accent = newValue }
    }

    init(action: BridgeCandidateAction, accent: NSColor) {
        self.action = action
        bar = ProgressTrackNSView(progress: 0, accent: accent)
        super.init(frame: .zero)
        label.font = .systemFont(ofSize: NSFont.systemFontSize)
        label.lineBreakMode = .byTruncatingTail
        let stack = NSStackView(views: [label, bar])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = ThemeSpace.inline
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            bar.widthAnchor.constraint(equalTo: stack.widthAnchor),
            bar.heightAnchor.constraint(
                equalToConstant: ProgressTrackNSView.trackHeight
            ),
        ])
        show(nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    func follow(_ progress: LatestSource<BridgeSelectionActionProgress>) {
        following.follow(progress)
    }

    func stopFollowing() {
        following.stop()
    }

    /// Before the action counts its folders, it names none and fills
    /// nothing.
    private func show(_ progress: BridgeSelectionActionProgress?) {
        let total = Int(progress?.total ?? 0)
        label.stringValue = action.label(count: total)
        bar.progress =
            total > 0
            ? Double(progress?.completed ?? 0) / Double(total) : 0
    }
}

/// SwiftUI's place for a bulk action's progress: it hands the native line its
/// action and stream, never a value.
struct SelectionActionProgressLine: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let action: BridgeCandidateAction
    let progress: LatestSource<BridgeSelectionActionProgress>

    func makeNSView(context _: Context) -> SelectionActionProgressNSView {
        let view = SelectionActionProgressNSView(action: action, accent: fill)
        view.follow(progress)
        return view
    }

    func updateNSView(
        _ view: SelectionActionProgressNSView,
        context _: Context
    ) {
        view.accent = fill
        view.follow(progress)
    }

    static func dismantleNSView(
        _ view: SelectionActionProgressNSView,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }

    private var fill: NSColor {
        NSColor(accent.color(in: colorScheme))
    }
}
