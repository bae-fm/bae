import SwiftUI

extension Font {
    /// The 14 pt medium title shared by every browse row (master list names,
    /// work / release / recording titles).
    static let browseRowTitle = Font.system(
        size: 14,
        weight: .medium
    )
    /// The 11.5 pt secondary line under a browse row's title (counts, credits,
    /// formats, album names).
    static let browseRowCaption = Font.system(size: 11.5)
    /// The 15 pt bold section label ("Works", "Releases", "Recordings",
    /// "Credits").
    static let browseSectionLabel = Font.system(
        size: 15,
        weight: .bold
    )
}
