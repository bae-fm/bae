using System.Collections.Generic;
using System.Linq;

namespace Bae.Windows;

/// <summary>
/// One candidate identity from <c>bae_search_releases</c>. The re-identify
/// picker renders <see cref="Summary"/> and passes <see cref="ReleaseId"/> +
/// <see cref="Source"/> back to <c>bae_reidentify_release</c> to commit.
/// </summary>
public sealed class Candidate
{
    public string Source { get; set; } = string.Empty;
    public string ReleaseId { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string? Artist { get; set; }
    public int? Year { get; set; }
    public string? Format { get; set; }
    public string? Label { get; set; }
    public string? CatalogNumber { get; set; }
    public string? Country { get; set; }

    /// <summary>The one-line label the picker shows, omitting absent fields.</summary>
    public string Summary
    {
        get
        {
            var parts = new[] { Artist, Title, Year?.ToString(), Format, Label, Country, CatalogNumber };
            return string.Join("  ·  ", parts.Where(part => !string.IsNullOrEmpty(part)));
        }
    }
}
