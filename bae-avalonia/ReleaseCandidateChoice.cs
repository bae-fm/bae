using System;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>A release pressing choice shown by import and re-identify pickers.
/// A row is one pressing however many sources carry it, and core settles what
/// picking it claims — the lead's facts fill the row's label, and
/// <see cref="Provenance"/> is the claim itself.</summary>
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
    internal string Title => _group.Title;
    internal string? Artist => _group.Artist;

    /// <summary>What picking this row claims, as core settled it.</summary>
    internal BridgeMetadataProvenance Provenance => _pressing.Pick;

    /// <summary>The same claim, in the shape a release already in the library
    /// takes.</summary>
    internal BridgeReleaseReseed Reseed => Provenance switch
    {
        BridgeMetadataProvenance.ExternalRelease external =>
            new BridgeReleaseReseed.ExternalRelease(
                external.ReleaseId, external.Source, external.Partners),
        BridgeMetadataProvenance.FileTags => new BridgeReleaseReseed.FileTags(),
        _ => throw new ArgumentOutOfRangeException(
            nameof(Provenance), Provenance, "Unknown metadata provenance"),
    };

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
