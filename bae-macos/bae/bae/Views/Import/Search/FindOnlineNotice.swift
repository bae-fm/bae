import SwiftUI

/// The result area when there is nothing to list: what happened on one line,
/// and what to do about it on the next. The form below stays where it is, so
/// the second line points at it rather than repeating its controls.
struct FindOnlineNotice: View {
    let title: LocalizedStringKey
    let detail: LocalizedStringKey

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(detail)
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 22)
        .frame(
            maxWidth: .infinity,
            maxHeight: .infinity,
            alignment: .topLeading
        )
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Nothing found") {
        FindOnlineNotice(
            title: "Neither source knows these signals.",
            detail:
                "Small pressings and reissues often lack a Disc ID entry. Searching by artist and album usually finds them."
        )
        .frame(width: 620, height: 160)
        .windowBackground()
    }
#endif
