/// The range assistive clients read and set on the playback sliders.
enum SliderAccessibilityScale {
    static let percentRange = 0.0...100.0

    static func percent(for fraction: Double) -> Double {
        (fraction * percentRange.upperBound).rounded()
    }

    static func fraction(for percent: Double) -> Double {
        max(0, min(1, percent / percentRange.upperBound))
    }
}
