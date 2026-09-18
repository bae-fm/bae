namespace Bae.Desktop;

/// <summary>
/// The two sizes the release's facts are drawn at: full width in the import
/// pane, and packed into the card behind a candidate row's glyphs or the
/// library expansion's facts line.
///
/// The mark and rip-match lines read the same at both; what the card packs is
/// the space between lines and the records row, which sits in a 320-point card
/// rather than across a pane.
/// </summary>
internal enum ReleaseFactsScale
{
    Pane,
    Card,
}

internal static class ReleaseFactsScaleMetrics
{
    /// <summary>The gap between one mark line and the next, and between the
    /// last mark and the rip-match line.</summary>
    internal static double LineSpacing(this ReleaseFactsScale scale) => scale switch
    {
        ReleaseFactsScale.Pane => 7,
        ReleaseFactsScale.Card => 6,
        _ => throw new System.ArgumentOutOfRangeException(nameof(scale), scale, null),
    };

    /// <summary>The gap between two record links on one row.</summary>
    internal static double RecordSpacing(this ReleaseFactsScale scale) => scale switch
    {
        ReleaseFactsScale.Pane => 14,
        ReleaseFactsScale.Card => 12,
        _ => throw new System.ArgumentOutOfRangeException(nameof(scale), scale, null),
    };

    /// <summary>The gap between two rows of record links.</summary>
    internal static double RecordRowSpacing(this ReleaseFactsScale scale) => scale switch
    {
        ReleaseFactsScale.Pane => 6,
        ReleaseFactsScale.Card => 4,
        _ => throw new System.ArgumentOutOfRangeException(nameof(scale), scale, null),
    };

    internal static double RecordFontSize(this ReleaseFactsScale scale) => scale switch
    {
        ReleaseFactsScale.Pane => 11.5,
        ReleaseFactsScale.Card => 11,
        _ => throw new System.ArgumentOutOfRangeException(nameof(scale), scale, null),
    };
}
