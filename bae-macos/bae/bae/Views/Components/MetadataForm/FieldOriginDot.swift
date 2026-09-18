import BaeKit
import SwiftUI

/// The dot after one release field's value, saying that the value is the
/// person's own or that the catalogs describing the release do not agree on
/// it. Which of the two it says — and whether it says anything at all — is
/// core's answer; this draws it and hovers the readings behind it.
struct FieldOriginDot: View {
    let provenance: BridgeFieldProvenance

    static let diameter: CGFloat = 5

    var body: some View {
        if let dot = provenance.dot {
            Circle()
                .fill(tint(dot))
                .frame(width: Self.diameter, height: Self.diameter)
                // A shape draws nothing an assistive technology can reach, so
                // the dot has to say it is an element before it can be named.
                .accessibilityElement()
                .accessibilityIdentifier(
                    "origin-dot-\(bridgeFieldName(field: provenance.field))"
                )
                .hoverPopover(arrowEdge: .bottom) {
                    FieldOriginPopover(provenance: provenance)
                        .popoverEntrance(anchor: .top)
                        .background { PopoverBehavior() }
                }
        }
    }

    private func tint(_ dot: BridgeFieldDot) -> AnyShapeStyle {
        switch dot {
        case .typed: AnyShapeStyle(.secondary)
        case .disagreement: AnyShapeStyle(Theme.accent)
        }
    }
}

/// What stands behind one field's dot: a line per catalog describing the
/// release with what that catalog states, and a line for where the value in
/// the field itself came from when it was not a catalog.
struct FieldOriginPopover: View {
    let provenance: BridgeFieldProvenance

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(provenance.claims, id: \.catalog) { claim in
                line(
                    bridgeCatalogName(catalog: claim.catalog),
                    // An editable value's blank is the person's to fill; a
                    // catalog stating nothing is a fact about the catalog, and
                    // the dash is how the grid writes it everywhere else.
                    claim.value ?? "\u{2014}"
                )
            }
            if let origin = provenance.origin {
                switch origin {
                case .typed:
                    Text(coreString("core.field.origin.typed"))
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                case .tags:
                    Text(coreString("core.field.origin.tags"))
                        .font(.system(size: 11.5))
                        .foregroundStyle(.secondary)
                // The catalog's own line already says what it states, so the
                // value being read from it adds nothing.
                case .record: EmptyView()
                }
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .frame(width: 240, alignment: .leading)
        .accessibilityIdentifier(
            "origin-lines-\(bridgeFieldName(field: provenance.field))"
        )
    }

    private func line(_ catalog: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(verbatim: catalog)
                .font(.system(size: 11.5, weight: .medium))
                .foregroundStyle(.secondary)
            Spacer(minLength: 0)
            Text(verbatim: value)
                .font(.system(size: 11.5))
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Two catalogs disagreeing") {
        FieldOriginPopover(
            provenance: PreviewData.disagreeingCatalogNumber
        )
        .importPreviewEnvironment()
    }

    #Preview("Typed over the tags") {
        FieldOriginPopover(provenance: PreviewData.typedLabel)
            .importPreviewEnvironment()
    }
#endif
