namespace Bae.Desktop;

/// <summary>
/// The two sizes a release's records row is drawn at: full width in the import
/// pane, and packed into the card the library expansion's facts line opens,
/// which sits in a 320-point card rather than across a pane.
/// </summary>
internal enum ReleaseFactsScale
{
    Pane,
    Card,
}

internal static class ReleaseFactsScaleMetrics
{
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
