namespace Bae.Windows;

/// <summary>What a restore code decodes to.</summary>
public sealed class RestoreCodeInfo
{
    public string LibraryId { get; set; } = string.Empty;
    public string LibraryName { get; set; } = string.Empty;
    public string Provider { get; set; } = string.Empty;
    public bool NeedsOauth { get; set; }
}
