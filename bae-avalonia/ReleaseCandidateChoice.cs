using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>A release pressing choice shown by import and re-identify pickers.
/// A row is one pressing however many sources carry it: picking it reads the
/// draft from the lead's release and claims every other source's record of the
/// same pressing alongside it.</summary>
public sealed class ReleaseCandidateChoice
{
    private readonly BridgeReleaseGroup _group;
    private readonly BridgePressing _pressing;
    private readonly BridgeMetadataResult _lead;

    internal ReleaseCandidateChoice(BridgeReleaseGroup group, BridgePressing pressing)
    {
        _group = group;
        _pressing = pressing;
        _lead = pressing.Releases[0];
    }

    internal BridgeMetadataSource Source => _lead.Source;
    public string ReleaseId => _lead.ReleaseId;

    /// <summary>Every source's record of this pressing other than the lead's.
    /// </summary>
    private BridgeMetadataRef[] Partners =>
        _pressing.Releases
            .Skip(1)
            .Select(release => new BridgeMetadataRef(release.Source, release.ReleaseId))
            .ToArray();

    /// <summary>What picking this row claims for an import candidate.</summary>
    internal BridgeMetadataProvenance Provenance =>
        new BridgeMetadataProvenance.ExternalRelease(Source, ReleaseId, Partners);

    /// <summary>The same claim for a release already in the library.</summary>
    internal BridgeReleaseReseed Reseed =>
        new BridgeReleaseReseed.ExternalRelease(ReleaseId, Source, Partners);

    /// <summary>The one-line label the picker shows, omitting absent fields.</summary>
    public string Summary
    {
        get
        {
            var parts = new[]
            {
                _group.Artist,
                _group.Title,
                _lead.Year?.ToString(),
                _lead.Format,
                _lead.Label,
                _lead.Country,
                _lead.CatalogNumber,
            };
            return string.Join("  ·  ", parts.Where(part => !string.IsNullOrEmpty(part)));
        }
    }
}
