using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>What the mapping pane has already made of the candidate under it,
/// as far as seeding that candidate's release goes. Every field is a fact about
/// the pane, not about the row.</summary>
/// <param name="Seeded">A prefetched edit is in — the user picked a release, or
/// took the import that claims nothing. Either way the pane is showing an
/// answer, and a seed would overwrite it.</param>
/// <param name="Prefetching">A prefetch is in flight. The pick it will land is
/// not readable yet, so a second seed would run the same work twice.</param>
/// <param name="PrefetchFailed">A prefetch for this candidate failed and said
/// so. Without this the failure would seed itself again on the next store tick,
/// and on a persistent failure, forever.</param>
internal readonly record struct MappingPaneSeedState(
    bool Seeded,
    bool Prefetching,
    bool PrefetchFailed);

/// <summary>The release a selected row opens the mapping pane on without a
/// click.</summary>
internal static class ReadyAutoPick
{
    /// <summary>The settled match the row leads with, or null when there is
    /// nothing to seed — the pane has already made something of this candidate,
    /// or the row isn't Ready.
    ///
    /// The placement gate carries weight the match alone can't. Done and Skipped
    /// rows hold `Matched` too, so without it re-showing an imported folder would
    /// re-open a commit-able pane; and a Needs-you row with several matches leads
    /// with one of them while core deliberately withholds the pressing, so
    /// seeding it would answer the question the row is asking.</summary>
    internal static BridgeMatchedRelease? From(
        BridgeTriageRow? row,
        MappingPaneSeedState pane)
    {
        if (pane.Seeded || pane.Prefetching || pane.PrefetchFailed)
        {
            return null;
        }
        return row is { Placement: BridgeTriagePlacement.Ready } ? row.Matched : null;
    }
}
