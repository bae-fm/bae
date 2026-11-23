/// The whole percentages assistive clients read on progress bars and sliders.
enum AccessibilityPercentage {
    static let range = 0.0...100.0

    static func percent(for fraction: Double) -> Double {
        (fraction * range.upperBound).rounded()
    }

    static func fraction(for percent: Double) -> Double {
        max(0, min(1, percent / range.upperBound))
    }
}
