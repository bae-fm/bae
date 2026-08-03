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

/// <summary>The identity a selected row opens the mapping pane on without a
/// click — the settled single match, or the choice made earlier, both read
/// back off the stored row.</summary>
internal static class PickedResume
{
    /// <summary>The row's decided identity, or null when there is nothing to
    /// apply — the pane has already made something of this candidate, nothing
    /// was ever decided, or the row is past deciding.
    ///
    /// The placement gate carries weight the decided check can't. Done and
    /// Skipped rows keep their pick too, and after a restart their import
    /// status is not in the session, so without it re-showing an imported
    /// folder would re-open a commit-able pane.</summary>
    internal static BridgeIdentityPick? From(
        BridgeTriageRow? row,
        MappingPaneSeedState pane)
    {
        if (pane.Seeded || pane.Prefetching || pane.PrefetchFailed)
        {
            return null;
        }
        return row?.Placement
            is BridgeTriagePlacement.Ready or BridgeTriagePlacement.NeedsYou
            ? row.Picked
            : null;
    }
}
