using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>A release pressing choice shown by import and re-identify pickers.</summary>
public sealed class ReleaseCandidateChoice
{
    private readonly BridgeReleaseGroup _group;
    private readonly BridgeMetadataResult _pressing;

    internal ReleaseCandidateChoice(BridgeReleaseGroup group, BridgeMetadataResult pressing)
    {
        _group = group;
        _pressing = pressing;
    }

    internal BridgeMetadataSource Source => _pressing.Source;
    public string ReleaseId => _pressing.ReleaseId;

    /// <summary>The one-line label the picker shows, omitting absent fields.</summary>
    public string Summary
    {
        get
        {
            var parts = new[]
            {
                _group.Artist,
                _group.Title,
                _pressing.Year?.ToString(),
                _pressing.Format,
                _pressing.Label,
                _pressing.Country,
                _pressing.CatalogNumber,
            };
            return string.Join("  ·  ", parts.Where(part => !string.IsNullOrEmpty(part)));
        }
    }
}
