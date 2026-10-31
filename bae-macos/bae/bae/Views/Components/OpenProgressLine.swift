import AppKit
import BaeKit
import SwiftUI

/// The step a library's open runs, over a bar of how much of it is done,
/// set straight from the open's stream once per display frame. Between steps
/// it is hidden and tells `onStepping`, so SwiftUI swaps what it shows only
/// as a step starts or ends, never as one moves.
final class OpenProgressNSView: NSView {
    private let label = NSTextField(labelWithString: "")
    private let bar: ProgressTrackNSView
    private let stack: NSStackView
    private lazy var following = FrameFollowing<BridgeOpenProgress>(
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

    /// Follow `progress`, telling `onStepping` whether a step runs the next
    /// time the line shows, whatever it last told: a new SwiftUI place, or a
    /// new open, starts from nothing told.
    func follow(_ progress: LatestSource<BridgeOpenProgress>) {
        told = nil
        following.follow(progress)
    }

    func stopFollowing() {
        following.stop()
    }

    private func show(_ progress: BridgeOpenProgress?) {
        if let progress {
            let title = localizedCoreString(
                bridgeOpenStepKey(step: progress.step)
            )
            if label.stringValue != title {
                label.stringValue = title
            }
            bar.progress =
                progress.total > 0
                ? Double(progress.done) / Double(progress.total) : 0
        }
        let stepping = progress != nil
        isHidden = !stepping
        guard stepping != told else { return }
        told = stepping
        onStepping(stepping)
    }
}

/// SwiftUI's place for an open's steps: it hands the native line the open's
/// stream, never a value, and takes no room between steps.
private struct OpenProgressBar: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let steps: LatestSource<BridgeOpenProgress>
    let onStepping: (Bool) -> Void

    func makeNSView(context _: Context) -> OpenProgressNSView {
        let view = OpenProgressNSView(accent: fill)
        view.onStepping = onStepping
        view.follow(steps)
        return view
    }

    func updateNSView(_ view: OpenProgressNSView, context _: Context) {
        view.accent = fill
        view.onStepping = onStepping
        view.follow(steps)
    }

    static func dismantleNSView(
        _ view: OpenProgressNSView,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: OpenProgressNSView,
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

/// What shows while a library opens: `idle` until the open runs a step,
/// then the step's name over a bar of how much of it is done, in its place.
/// With no open to follow, `idle` alone.
struct OpenProgressLine<Idle: View>: View {
    let steps: LatestSource<BridgeOpenProgress>?
    let idle: Idle
    @State
    private var stepping = false

    init(
        steps: LatestSource<BridgeOpenProgress>?,
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
                OpenProgressBar(steps: steps) { started in
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
        OpenProgressLine(
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
            ProgressView("Loading...")
        }
        .frame(width: 320)
        .padding()
    }

    #Preview("Rebuilding") {
        OpenProgressLine(
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
            ProgressView("Loading...")
        }
        .frame(width: 320)
        .padding()
    }

    #Preview("Between steps") {
        OpenProgressLine(steps: LatestStream.openProgress().only) {
            ProgressView("Loading...")
        }
        .frame(width: 320)
        .padding()
    }
#endif

/// How wide an open's line is where it stands alone, on a loading screen.
enum OpenProgressLayout {
    static let width: CGFloat = 360
}
