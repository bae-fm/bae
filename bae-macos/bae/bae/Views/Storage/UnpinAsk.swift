import BaeKit
import SwiftUI

/// What Unpin does on this Mac, decided from the "Ask before unpinning"
/// setting and the person's answer to the alert: whether to ask at all, and
/// which calls to send afterwards.
enum UnpinAsk {
    /// One call Unpin sends, in the order it sends them.
    enum Step: Equatable {
        /// Turn "Ask before unpinning" off.
        case stopAsking
        /// Unpin the releases.
        case unpin
    }

    /// How the person closed the alert.
    enum Answer: Equatable {
        /// Unpin was chosen, with the "Don't show this again" box ticked or not.
        case unpin(dontShowAgain: Bool)
        /// Cancel was chosen. The box changes nothing: nothing was confirmed.
        case cancel
    }

    /// Whether Unpin shows the alert before doing anything.
    static func asks(askBeforeUnpinning: Bool) -> Bool {
        askBeforeUnpinning
    }

    /// What Unpin sends when it does not ask.
    static let withoutAsking: [Step] = [.unpin]

    /// What Unpin sends after the alert closed with `answer`.
    static func steps(after answer: Answer) -> [Step] {
        switch answer {
        case .unpin(dontShowAgain: true):
            [.stopAsking, .unpin]
        case .unpin(dontShowAgain: false):
            [.unpin]
        case .cancel:
            []
        }
    }

    /// Send `steps` in order, stopping at the first that throws: a setting
    /// that could not be saved leaves the releases pinned, and the error
    /// reaches whoever started the Unpin.
    static func run(
        _ steps: [Step],
        stopAsking: @Sendable () async throws -> Void,
        unpin: @Sendable () async throws -> Void
    ) async throws {
        for step in steps {
            switch step {
            case .stopAsking: try await stopAsking()
            case .unpin: try await unpin()
            }
        }
    }
}

/// The releases an Unpin alert is asking about.
struct PendingUnpin: Equatable {
    let releaseIds: [String]
    /// The album's title when the alert is about one release.
    let title: String?

    /// The alert's title: the release by name when it is one, else how many.
    var alertTitle: String {
        if let title, releaseIds.count == 1 {
            return String(localized: "Unpin “\(title)”?")
        }
        return String(localized: "Unpin \(releaseIds.count) releases?")
    }

    var alertMessage: String {
        releaseIds.count == 1
            ? String(
                localized:
                    "Its files will be removed from this Mac now. It stays in the cloud, and plays from there while you’re online."
            )
            : String(
                localized:
                    "Their files will be removed from this Mac now. They stay in the cloud, and play from there while you’re online."
            )
    }
}

/// The alert that asks before Unpin removes releases' files from this Mac,
/// with a box that stops it asking. It shows while `pending` names releases;
/// either button clears `pending`, and `onAnswer` hears how it closed.
struct UnpinConfirmation: ViewModifier {
    @Binding
    var pending: PendingUnpin?
    let onAnswer: (PendingUnpin, UnpinAsk.Answer) -> Void

    /// The suppression box, which SwiftUI writes whichever button closes the
    /// alert; the Unpin button reads it and Cancel ignores it.
    @State
    private var dontShowAgain = false

    func body(content: Content) -> some View {
        content
            .alert(
                Text(verbatim: pending?.alertTitle ?? ""),
                isPresented: Binding(
                    get: { pending != nil },
                    set: { presented in
                        if !presented {
                            pending = nil
                            dontShowAgain = false
                        }
                    }
                ),
                presenting: pending
            ) { pending in
                Button("Unpin") {
                    onAnswer(pending, .unpin(dontShowAgain: dontShowAgain))
                }
                Button("Cancel", role: .cancel) {
                    onAnswer(pending, .cancel)
                }
            } message: { pending in
                Text(verbatim: pending.alertMessage)
            }
            .dialogSuppressionToggle(
                "Don’t show this again",
                isSuppressed: $dontShowAgain
            )
    }
}

extension View {
    /// Ask before Unpin removes the releases `pending` names; see
    /// `UnpinConfirmation`.
    func unpinConfirmation(
        pending: Binding<PendingUnpin?>,
        onAnswer: @escaping (PendingUnpin, UnpinAsk.Answer) -> Void
    ) -> some View {
        modifier(UnpinConfirmation(pending: pending, onAnswer: onAnswer))
    }
}

#if DEBUG
    private struct UnpinConfirmationPreview: View {
        let pending: PendingUnpin
        @State
        private var shown: PendingUnpin?

        var body: some View {
            Button {
                shown = pending
            } label: {
                Text(verbatim: "Ask Again")
            }
            .frame(width: 480, height: 320)
            .unpinConfirmation(pending: $shown) { _, _ in }
            .onAppear { shown = pending }
        }
    }

    #Preview("Unpin One Release") {
        UnpinConfirmationPreview(
            pending: PendingUnpin(
                releaseIds: ["release"],
                title: "Low Tide Hours"
            )
        )
    }

    #Preview("Unpin Several Releases") {
        UnpinConfirmationPreview(
            pending: PendingUnpin(
                releaseIds: ["one", "two", "three"],
                title: nil
            )
        )
    }
#endif
