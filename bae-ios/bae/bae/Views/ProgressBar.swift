import BaeKit
import Combine
import SwiftUI
import UIKit

/// UIKit-backed playback progress leaf. The compact and expanded players pass
/// the high-frequency position subject here so ticks mutate UIKit controls
/// directly instead of invalidating SwiftUI view state.
struct ProgressBar: UIViewRepresentable {
    let positionSubject: CurrentValueSubject<PlaybackPositionEvent, Never>
    let onSeek: (Double) -> Void

    func makeUIView(context _: Context) -> PlaybackProgressUIView {
        let view = PlaybackProgressUIView()
        view.onSeek = onSeek
        view.subscribe(to: positionSubject)
        return view
    }

    func updateUIView(_ view: PlaybackProgressUIView, context _: Context) {
        view.onSeek = onSeek
        view.subscribe(to: positionSubject)
    }
}

final class PlaybackProgressUIView: UIView {
    private let elapsedLabel = UILabel()
    private let slider = UISlider()
    private let remainingLabel = UILabel()

    var onSeek: (Double) -> Void = { _ in }

    private var isDragging = false
    private var subject: CurrentValueSubject<PlaybackPositionEvent, Never>?
    private var cancellable: AnyCancellable?

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureTimeLabel(elapsedLabel)
        configureTimeLabel(remainingLabel)

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
            elapsedLabel,
            slider,
            remainingLabel,
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

    func subscribe(to subject: CurrentValueSubject<PlaybackPositionEvent, Never>) {
        if self.subject === subject {
            return
        }
        self.subject = subject
        cancellable =
            subject
            .receive(on: DispatchQueue.main)
            .sink { [weak self] event in
                self?.apply(event)
            }
    }

    private func apply(_ event: PlaybackPositionEvent) {
        switch event {
        case .position(let progress, let elapsed, let remaining):
            elapsedLabel.text = elapsed
            remainingLabel.text = remaining
            if !isDragging {
                slider.value = Float(max(0, min(1, progress)))
            }
        case .reset:
            isDragging = false
            slider.value = 0
            elapsedLabel.text = ""
            remainingLabel.text = ""
        }
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
