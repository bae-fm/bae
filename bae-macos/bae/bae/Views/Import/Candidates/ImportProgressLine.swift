import AppKit
import BaeKit
import SwiftUI

/// A running import's line under a triage row: the phase it is in and the
/// bar. The bar is how far through it is; a number beside it would say the
/// same thing again in a row that has no room for it.
///
/// The phase is the candidate's runtime, which changes as the import steps;
/// the bar is set straight from the import percents' stream once per
/// display frame, so a percent tick redraws the bar and runs no SwiftUI
/// update. The row itself only says *that* an import is running, which is a
/// fact the queue projects.
struct ImportProgressLine: View {
    let key: String
    @Environment(\.importPercents)
    private var percents

    var body: some View {
        CandidateRuntimeReader(key: key) { runtime in
            ProgressLine(
                phaseText(step: runtime?.import?.step),
                bar: ImportPercentBar(percent: percents?.source(for: key))
            )
        }
    }

    /// A row placed as importing whose run has not reported yet is at the
    /// start with no phase named.
    private func phaseText(step: BridgeImportStep?) -> String {
        step?.localizedText ?? String(localized: "Importing\u{2026}")
    }
}

extension EnvironmentValues {
    /// How far through its step each running import is, for the native bars
    /// that read it each display frame; `nil` where no import runs.
    @Entry
    var importPercents: LatestStream<UInt32>?
}

/// A bar set straight from one candidate's import percent each display frame:
/// the marching pill until the import's step counts a percent.
final class ImportPercentBarNSView: ProgressTrackNSView {
    private lazy var following = FrameFollowing<UInt32>(view: self) {
        [weak self] percent in
        self?.progress = percent.map { Double($0) / 100 }
    }

    func follow(_ percent: LatestSource<UInt32>?) {
        guard let percent else {
            following.stop()
            progress = nil
            return
        }
        following.follow(percent)
    }

    func stopFollowing() {
        following.stop()
    }
}

/// SwiftUI's place for an import's bar: it hands the bar its stream, never a
/// value.
struct ImportPercentBar: NSViewRepresentable {
    @Environment(\.accentChoice)
    private var accent
    @Environment(\.colorScheme)
    private var colorScheme
    let percent: LatestSource<UInt32>?

    func makeNSView(context _: Context) -> ImportPercentBarNSView {
        let view = ImportPercentBarNSView(progress: nil, accent: fill)
        view.follow(percent)
        return view
    }

    func updateNSView(_ view: ImportPercentBarNSView, context _: Context) {
        view.accent = fill
        view.follow(percent)
    }

    static func dismantleNSView(
        _ view: ImportPercentBarNSView,
        coordinator _: ()
    ) {
        view.stopFollowing()
    }

    private var fill: NSColor {
        NSColor(accent.color(in: colorScheme))
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView _: ImportPercentBarNSView,
        context _: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? 0,
            height: ProgressTrackNSView.trackHeight
        )
    }
}
