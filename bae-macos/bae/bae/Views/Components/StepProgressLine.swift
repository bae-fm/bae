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
    var title: String { localizedCoreString(bridgeOpenStepKey(step: step)) }
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
            localizedCoreString(bridgeLoadStepKey(step: progress.step))
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

extension BridgeClosingStep: ProgressStep {
    var title: String { localizedCoreString(bridgeClosingStepKey(step: self)) }
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
        localizedCoreString(bridgeStorageSetupStepKey(step: step))
    }
    /// A step of one unit moves without a count; loading's steps count.
    var fraction: Double? {
        total > 1 ? Double(done) / Double(total) : nil
    }
}

extension BridgeSetupProgress: ProgressStep {
    var title: String { localizedCoreString(bridgeSetupStepKey(step: step)) }
    var fraction: Double? {
        total > 0 ? Double(done) / Double(total) : nil
    }
}

/// The step a library's open, a setup call, a close or an apply of a catalog
/// release runs, over a bar of how much of it is done, set straight from the
/// call's stream once per display frame. Between steps it is hidden and tells `onStepping`, so SwiftUI
/// swaps what it shows only as a step starts or ends, never as one moves.
final class StepProgressNSView<Step: ProgressStep>: NSView {
    private let label = NSTextField(labelWithString: "")
    private let bar: ProgressTrackNSView
    private let stack: NSStackView
    private lazy var following = FrameFollowing<Step>(
        view: self
    ) { [weak self] progress in self?.show(progress) }
    var onStepping: (Bool) -> Void = { _ in }
    /// Whether a step runs, as last told to `onStepping`; nil until the
    /// line shows what the stream it follows holds.
    private var told: Bool?
    var stepping: Bool { told ?? false }

    var accent: NSColor {
        get { bar.accent }
        set { bar.accent = newValue }
    }

    init(accent: NSColor) {
        bar = ProgressTrackNSView(progress: 0, accent: accent)
        stack = NSStackView(views: [label, bar])
        super.init(frame: .zero)
        label.font = .systemFont(ofSize: NSFont.systemFontSize)
        label.textColor = .secondaryLabelColor
        label.lineBreakMode = .byTruncatingTail
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = ThemeSpace.inline
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            bar.widthAnchor.constraint(equalTo: stack.widthAnchor),
            bar.heightAnchor.constraint(
                equalToConstant: ProgressTrackNSView.trackHeight
            ),
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

    /// Follow `progress`. A new stream tells `onStepping` whether a step
    /// runs as soon as the line shows what it holds, whatever was told
    /// before; the stream already followed keeps what was told, which
    /// SwiftUI's next size of the line reads.
    func follow(_ progress: LatestSource<Step>) {
        let toldBefore = told
        told = nil
        following.follow(progress)
        // Following a stream shows what it holds at once, which tells;
        // the one already followed shows nothing anew.
        if told == nil {
            told = toldBefore
        }
    }

    func stopFollowing() {
        following.stop()
    }

    private func show(_ progress: Step?) {
        if let progress {
            let title = progress.title
            if label.stringValue != title {
                label.stringValue = title
            }
            bar.progress = progress.fraction
        }
        let stepping = progress != nil
        isHidden = !stepping
        guard stepping != told else { return }
        told = stepping
        onStepping(stepping)
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
    let onStepping: (Bool) -> Void

    func makeNSView(context _: Context) -> StepProgressNSView<Step> {
        let view = StepProgressNSView<Step>(accent: fill)
        view.onStepping = onStepping
        view.follow(steps)
        return view
    }

    func updateNSView(_ view: StepProgressNSView<Step>, context _: Context) {
        view.accent = fill
        view.onStepping = onStepping
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

/// What shows while a library opens or a setup call runs: `idle` until the
/// call runs a step, then the step's name over a bar of how much of it is
/// done, in its place. With no call to follow, `idle` alone.
struct StepProgressLine<Step: ProgressStep, Idle: View>: View {
    let steps: LatestSource<Step>?
    let idle: Idle
    @State
    private var stepping = false

    init(
        steps: LatestSource<Step>?,
        @ViewBuilder idle: () -> Idle
    ) {
        self.steps = steps
        self.idle = idle()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !stepping || steps == nil {
                idle
            }
            if let steps {
                StepProgressBar(steps: steps) { started in
                    // The line can start following during a SwiftUI
                    // update, which mustn't change state itself.
                    DispatchQueue.main.async { stepping = started }
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Migrating") {
        StepProgressLine(
            steps:
                LatestStream.openProgress(
                    holding: BridgeOpenProgress(
                        step: .migrating,
                        done: 1,
                        total: 3
                    )
                )
                .only
        ) {
            ProgressView(String(localized: AppScreen.LoadingWork.opening.title))
        }
        .frame(width: 320)
        .padding()
    }

    #Preview("Rebuilding") {
        StepProgressLine(
            steps:
                LatestStream.openProgress(
                    holding: BridgeOpenProgress(
                        step: .rebuilding,
                        done: 4,
                        total: 9
                    )
                )
                .only
        ) {
            ProgressView(String(localized: AppScreen.LoadingWork.opening.title))
        }
        .frame(width: 320)
        .padding()
    }

    #Preview("Removing libraries") {
        StepProgressLine(
            steps:
                LatestStream.setupProgress(
                    holding: BridgeSetupProgress(
                        step: .removingLibraries,
                        done: 1,
                        total: 3
                    )
                )
                .only
        ) {
            EmptyView()
        }
        .frame(width: 320)
        .padding()
    }

    #Preview("Between steps") {
        StepProgressLine(steps: LatestStream.openProgress().only) {
            ProgressView(String(localized: AppScreen.LoadingWork.opening.title))
        }
        .frame(width: 320)
        .padding()
    }
#endif

/// How wide a call's line is where it stands alone, on a loading screen.
enum StepProgressLayout {
    static let width: CGFloat = 360
}
