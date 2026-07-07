using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>A release pressing choice shown by import and re-identify pickers.</summary>
public sealed record ReleaseCandidateChoice(BridgeReleaseGroup Group, BridgeMetadataResult Pressing)
{
    public BridgeMetadataSource Source => Pressing.Source;
    public string ReleaseId => Pressing.ReleaseId;

    /// <summary>The one-line label the picker shows, omitting absent fields.</summary>
    public string Summary
    {
        get
        {
            var parts = new[]
            {
                Group.Artist,
                Group.Title,
                Pressing.Year?.ToString(),
                Pressing.Format,
                Pressing.Label,
                Pressing.Country,
                Pressing.CatalogNumber,
            };
            return string.Join("  ·  ", parts.Where(part => !string.IsNullOrEmpty(part)));
        }
    }
}
