using System.Collections.Generic;

using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>One metadata pick, from its click until the read it started ends.
/// The store owns it under the candidate's key, so it is not the pane's to
/// lose when the pane looks at something else.</summary>
internal sealed class ImportMetadataPick
{
    internal ImportMetadataPick(BridgeMetadataProvenance provenance, string? audioIdentity)
    {
        Provenance = provenance;
        AudioIdentity = audioIdentity;
    }

    /// <summary>The source this pick is reading into the candidate's draft.</summary>
    internal BridgeMetadataProvenance Provenance { get; }

    /// <summary>The audio the folder held when the pick was made. Audio that
    /// changes under it leaves the pick a claim about a folder that is no
    /// longer there.</summary>
    internal string? AudioIdentity { get; }
}

internal sealed partial class ImportStore
{
    // What each candidate's metadata pick is reading, under that candidate's
    // key. Picking is a decision about one folder, so it lives here rather
    // than on the pane: looking at another candidate neither cancels the read
    // nor lets it land on whatever is on screen when it returns. An entry ends
    // when its own read ends, and is dropped only when the candidate stops
    // being a scanned folder or its audio changes underneath it.
    private readonly Dictionary<string, ImportMetadataPick> _picks = new();

    /// <summary>What this candidate's pick is reading, if it has one.</summary>
    public BridgeMetadataProvenance? PickInFlight(string key) =>
        _picks.TryGetValue(key, out var pick) ? pick.Provenance : null;

    /// <summary>Start one pick, before its bridge command is dispatched.
    /// Replacing a pick abandons the older one: only the pick the store holds
    /// can land.</summary>
    public ImportMetadataPick? BeginMetadataApplication(
        string key,
        BridgeMetadataProvenance provenance)
    {
        if (!_candidates.TryGetValue(key, out var candidate))
        {
            return null;
        }
        var pick = new ImportMetadataPick(provenance, candidate.Files?.FileTagsIdentity);
        _picks[key] = pick;
        Changed?.Invoke();
        return pick;
    }

    /// <summary>The pick landed: core holds the draft it read, so this
    /// candidate's pane goes back to the draft — whether or not it is the one
    /// being looked at.</summary>
    public void MetadataApplicationSucceeded(string key, ImportMetadataPick pick)
    {
        if (!EndMetadataApplication(key, pick))
        {
            return;
        }
        PresentMetadata(key, ImportMetadataPresentation.Draft);
    }

    /// <summary>End this pick if it is still the one in flight. A replacement
    /// may already own the candidate by the time an older read returns, and
    /// that newer pick is the one that gets to land.</summary>
    public bool EndMetadataApplication(string key, ImportMetadataPick pick)
    {
        if (!_picks.TryGetValue(key, out var current) || !ReferenceEquals(current, pick))
        {
            return false;
        }
        _picks.Remove(key);
        Changed?.Invoke();
        return true;
    }

    /// <summary>Drop this candidate's pick whatever it is reading: the folder
    /// it claimed is gone, or is no longer the folder it was made about.</summary>
    public void CancelMetadataApplication(string key)
    {
        if (_picks.Remove(key))
        {
            Changed?.Invoke();
        }
    }
}
