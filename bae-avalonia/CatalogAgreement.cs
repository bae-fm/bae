namespace Bae.Desktop;

/// <summary>
/// One catalog number the candidate's own text states about a release
/// identification is offering — a chip beside the signal badges. Counted, it
/// ranks that release up the list and badges its row; struck out, it counts
/// for nothing, which is what the numbers on a sleeve that are not catalog
/// numbers are for.
///
/// Core derives the set on every read of the candidate, so striking one out
/// re-ranks the answers in hand with nothing looked up again.
/// </summary>
public sealed class CatalogAgreement
{
    public string Value { get; set; } = string.Empty;

    /// <summary>Whether the person struck it out. The chip stands either way:
    /// struck out is a state to come back from.</summary>
    public bool Discounted { get; set; }
}
