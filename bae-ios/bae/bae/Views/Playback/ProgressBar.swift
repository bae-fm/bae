import BaeKit
import Combine
import SwiftUI
import UIKit

/// UIKit-backed playback progress leaf. The compact and expanded players pass
/// the high-frequency position publisher here so ticks mutate UIKit controls
/// directly instead of invalidating SwiftUI view state.
struct ProgressBar: UIViewRepresentable {
    let positionPublisher: AnyPublisher<PlaybackPositionEvent, Never>
    /// The user's elapsed-vs-remaining choice, from the config. Tapping the
    /// leading label writes the config; the invalidation lands the new value
    /// back here, which is what re-renders the bar.
    let showRemainingTime: Bool
    let onSeek: (Double) -> Void
    let onToggleRemainingTime: () -> Void

    func makeUIView(context _: Context) -> PlaybackProgressUIView {
        let view = PlaybackProgressUIView()
        apply(to: view)
        view.subscribe(to: positionPublisher)
        return view
    }

    func updateUIView(_ view: PlaybackProgressUIView, context _: Context) {
        apply(to: view)
        view.subscribe(to: positionPublisher)
    }

    private func apply(to view: PlaybackProgressUIView) {
        view.onSeek = onSeek
        view.onToggleRemainingTime = onToggleRemainingTime
        view.showRemainingTime = showRemainingTime
    }
}

final class PlaybackProgressUIView: UIView {
    private let leadingLabel = UILabel()
    private let slider = UISlider()
    private let trailingLabel = UILabel()

    var onSeek: (Double) -> Void = { _ in }
    var onToggleRemainingTime: () -> Void = {}

    /// The user's choice, from the config. The view stores no copy of it.
    var showRemainingTime = false {
        didSet {
            if showRemainingTime != oldValue {
                updateLabels()
            }
        }
    }

    private var isDragging = false
    private var positionMs: UInt64 = 0
    private var durationMs: UInt64 = 0
    private var cancellable: AnyCancellable?

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureTimeLabel(leadingLabel)
        configureTimeLabel(trailingLabel)

        // Tapping the leading label switches it between elapsed and remaining.
        leadingLabel.isUserInteractionEnabled = true
        leadingLabel.addGestureRecognizer(
            UITapGestureRecognizer(
                target: self,
                action: #selector(leadingLabelTapped)
            )
        )

        slider.minimumValue = 0
        slider.maximumValue = 1
        slider.isContinuous = true
        slider.translatesAutoresizingMaskIntoConstraints = false
        slider.addTarget(
            self,
            action: #selector(beginDragging),
            for: .touchDown
        )
        slider.addTarget(
            self,
            action: #selector(finishDragging),
            for: [.touchUpInside, .touchUpOutside, .touchCancel]
        )

        let stack = UIStackView(arrangedSubviews: [
            leadingLabel,
            slider,
            trailingLabel,
        ])
        stack.axis = .horizontal
        stack.alignment = .center
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    private func configureTimeLabel(_ label: UILabel) {
        label.font = .monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        label.textColor = .secondaryLabel
        label.textAlignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        label.widthAnchor.constraint(equalToConstant: 48).isActive = true
    }

    func subscribe(to publisher: AnyPublisher<PlaybackPositionEvent, Never>) {
        cancellable =
            publisher
            .receive(on: DispatchQueue.main)
            .sink { [weak self] event in
                self?.apply(event)
            }
    }

    private func apply(_ event: PlaybackPositionEvent) {
        switch event {
        case .position(let progress, let positionMs, let durationMs):
            self.positionMs = positionMs
            self.durationMs = durationMs
            updateLabels()
            if !isDragging {
                slider.value = Float(max(0, min(1, progress)))
            }
        case .reset:
            isDragging = false
            slider.value = 0
            positionMs = 0
            durationMs = 0
            leadingLabel.text = ""
            trailingLabel.text = ""
        }
    }

    /// Both labels come from one core projection, so the leading one can never
    /// disagree with the trailing one about which clock it is showing.
    private func updateLabels() {
        let labels = DurationClock.seekBar(
            positionMs: positionMs,
            durationMs: durationMs,
            showRemaining: showRemainingTime
        )
        leadingLabel.text = labels.leading
        trailingLabel.text = labels.trailing
    }

    @objc
    private func leadingLabelTapped() {
        onToggleRemainingTime()
    }

    @objc
    private func beginDragging() {
        isDragging = true
    }

    @objc
    private func finishDragging() {
        let ratio = Double(slider.value)
        isDragging = false
        onSeek(ratio)
    }
}

#if DEBUG
#Preview {
    ProgressBar(
        positionPublisher: PreviewData.playbackStore()
            .playbackPositionPublisher,
        showRemainingTime: false,
        onSeek: { _ in },
        onToggleRemainingTime: {}
    )
    .frame(height: 60)
    .padding()
}
#endif
