import AppKit
import BaeKit
import SwiftUI

/// A step a long call runs, as a progress line shows it: its name, and how
/// much of it is done, nil while there is nothing to count.
protocol ProgressStep: Sendable, Equatable {
    var title: String { get }
    var fraction: Double? { get }
}

extension BridgeOpenProgress: ProgressStep {
    var title: String { localizedCoreString(key) }
    var fraction: Double? {
        total > 0 ? Double(done) / Double(total) : 0
    }
}

extension BridgeBootstrapStep: ProgressStep {
    var title: String {
        switch self {
        case .opening(let progress):
            progress.title
        case .loading(let progress):
            localizedCoreString(progress.key)
        }
    }
    var fraction: Double? {
        switch self {
        case .opening(let progress):
            progress.fraction
        case .loading(let progress):
            Double(min(progress.done, progress.total))
                / Double(max(progress.total, 1))
        }
    }
}

extension BridgeClosingProgress: ProgressStep {
    var title: String { localizedCoreString(key) }
    /// A close's steps count nothing.
    var fraction: Double? { nil }
}

extension BridgeCatalogApplyStep: ProgressStep {
    var title: String { bridgeCatalogApplyStepLine(step: self).localizedLine }
    /// A read of a catalog counts nothing.
    var fraction: Double? { nil }
}

extension BridgeStorageSetupProgress: ProgressStep {
    var title: String {
        localizedCoreString(key)
    }
    /// A step of one unit moves without a count; loading's steps count.
    var fraction: Double? {
        total > 1 ? Double(done) / Double(total) : nil
    }
}

extension BridgeSetupProgress: ProgressStep {
    var title: String { localizedCoreString(key) }
    var fraction: Double? {
        total > 0 ? Double(done) / Double(total) : nil
    }
}

extension SetupLine: ProgressStep {
    var title: String {
        switch self {
        case .step(let progress): progress.title
        case .storage(let progress): progress.title
        }
    }
    var fraction: Double? {
        switch self {
        case .step(let progress): progress.fraction
        case .storage(let progress): progress.fraction
        }
    }
}

/// How a call's step shows. `line` is the name over its bar in a pane or
/// sheet, left-aligned in secondary text. `stage` is the line of a screen the
/// call fills: the name centred over the bar in primary text, the bar at the
/// design system's height. `note` is the name alone, as a row's note.
enum StepProgressLook {
    case line
    case stage
    case note
}

/// The step a library's open, a setup call, a close or an apply of a catalog
/// release runs, over a bar of how much of it is done, set straight from the
/// call's stream once per display frame. Between steps it is hidden. A step
/// starting or ending in the stream it follows tells `onStepping`, so SwiftUI
/// swaps what it shows only then, never as one moves.
final class StepProgressNSView<Step: ProgressStep>: NSView {
    private let label = NSTextField(labelWithString: "")
    private let bar: ProgressTrackNSView
    private let stack: NSStackView
    private lazy var following = FrameFollowing<Step>(
        view: self
    ) { [weak self] progress in self?.show(progress) }
    var onStepping: (Bool) -> Void = { _ in }
    /// The words a step shows under.
    var titled: (Step) -> String = { $0.title }
    /// Whether the line shows a step.
    private(set) var stepping = false
    /// Whether the line is starting to follow a stream, which shows what the
    /// stream holds during SwiftUI's update of the line.
    private var joining = false

    var accent: NSColor {
        get { bar.accent }
        set { bar.accent = newValue }
    }

    init(accent: NSColor, look: StepProgressLook = .line) {
        bar = ProgressTrackNSView(
            progress: 0,
            accent: accent,
            height: look == .stage
                ? ProgressTrackNSView.stageHeight
                : ProgressTrackNSView.trackHeight
        )
        bar.isHidden = look == .note
        stack = NSStackView(views: [label, bar])
        super.init(frame: .zero)
        switch look {
        case .line:
            label.font = .systemFont(ofSize: NSFont.systemFontSize)
            label.textColor = .secondaryLabelColor
        case .stage:
            label.font = .systemFont(ofSize: NSFont.systemFontSize)
            label.textColor = NSColor(Theme.textPrimary)
        case .note:
            label.font = ThemeText.detail.nsFont
            label.textColor = .secondaryLabelColor
        }
        label.lineBreakMode = .byTruncatingTail
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = look == .stage ? ThemeSpace.related : ThemeSpace.inline
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            bar.widthAnchor.constraint(equalTo: stack.widthAnchor),
            label.widthAnchor.constraint(lessThanOrEqualTo: stack.widthAnchor),
            bar.heightAnchor.constraint(equalToConstant: bar.height),
        ])
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    /// The line's height while a step runs.
    var steppingHeight: CGFloat {
        stack.fittingSize.height
    }

    /// Follow `progress`, showing at once what it holds. That show tells
    /// `onStepping` nothing: it runs in SwiftUI's update of the line, which
    /// mustn't change SwiftUI's state, and the line reads where a stream it
    /// starts to follow stands itself.
    func follow(_ progress: LatestSource<Step>) {
        joining = true
        following.follow(progress)
        joining = false
    }

    func stopFollowing() {
        following.stop()
    }

    private func show(_ progress: Step?) {
        if let progress {
            let title = titled(progress)
            if label.stringValue != title {
                label.stringValue = title
            }
            bar.progress = progress.fraction
        }
        let stepping = progress != nil
        isHidden = !stepping
        guard stepping != self.stepping else { return }
        self.stepping = stepping
        if !joining {
            onStepping(stepping)
        }
    }
}

/// SwiftUI's place for a call's steps: it hands the native line the call's
/// stream, never a value, and takes no room between steps.
private struct StepProgressBar<Step: ProgressStep>: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let steps: LatestSource<Step>
    let look: StepProgressLook
    let titled: (Step) -> String
    let onStepping: (Bool) -> Void

    func makeNSView(context _: Context) -> StepProgressNSView<Step> {
        let view = StepProgressNSView<Step>(accent: fill, look: look)
        view.onStepping = onStepping
        view.titled = titled
        view.follow(steps)
        return view
    }

    func updateNSView(_ view: StepProgressNSView<Step>, context _: Context) {
        view.accent = fill
        view.onStepping = onStepping
        view.titled = titled
        view.follow(steps)
    }

    static func dismantleNSView(
        _ view: StepProgressNSView<Step>,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: StepProgressNSView<Step>,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: nsView.stepping ? nsView.steppingHeight : 0
        )
    }

    private var fill: NSColor {
        NSColor(accent.color(in: colorScheme))
    }
}

/// A step's name over a bar, in the pane or sheet it runs in: the name in
/// secondary text, the bar 4pt high under it. Drawn from the values it is
/// given; a container reads the stream that moves the bar.
struct MacProgress<Bar: View>: View {
    let title: String
    let bar: Bar

    init(title: String, bar: Bar) {
        self.title = title
        self.bar = bar
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(title)
                .lineLimit(1)
                .truncationMode(.tail)
                .foregroundStyle(.secondary)
            bar
                .frame(height: ProgressTrackNSView.trackHeight)
        }
    }
}

/// Follows the step streams of a library's open and of setup calls, and draws
/// the step as `MacProgress` does, but natively: the step's name and its bar
/// are set straight from the stream once per display frame, so a moving
/// fraction runs no SwiftUI update. What shows while a library opens or a
/// setup call runs: `idle` until the
/// call runs a step, then the step's name over a bar of how much of it is
/// done, in its place; with the `note` look, the name alone, for a line whose
/// row shows a spinner already. `titled` words each step; by default its own
/// title. With no call to follow, `idle` alone. Whether a step
/// runs is read from the call's stream as the line starts to follow it, and
/// set as the native line tells a step starting or ending.
struct MacProgressContainer<Step: ProgressStep, Idle: View>: View {
    let steps: LatestSource<Step>?
    let look: StepProgressLook
    let titled: (Step) -> String
    let idle: Idle
    @State
    private var stepping: Bool

    init(
        steps: LatestSource<Step>?,
        look: StepProgressLook = .line,
        titled: @escaping (Step) -> String = { $0.title },
        @ViewBuilder idle: () -> Idle
    ) {
        self.steps = steps
        self.look = look
        self.titled = titled
        self.idle = idle()
        _stepping = State(initialValue: steps?.holdsValue ?? false)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !stepping || steps == nil {
                idle
            }
            if let steps {
                StepProgressBar(
                    steps: steps,
                    look: look,
                    titled: titled
                ) { started in
                    stepping = started
                }
            }
        }
        .onChange(of: steps) { _, steps in
            stepping = steps?.holdsValue ?? false
        }
    }
}

/// How wide a call's line is where it stands alone, on a loading screen.
enum StepProgressLayout {
    static let width: CGFloat = 360
}
