namespace Bae.Windows;

/// <summary>What a restore code decodes to (from <c>bae_decode_restore_code</c>).</summary>
public sealed class RestoreCodeInfo
{
    public string LibraryId { get; set; } = string.Empty;
    public string LibraryName { get; set; } = string.Empty;
    public string Provider { get; set; } = string.Empty;
    public bool NeedsOauth { get; set; }
}

/// <summary>The outcome of <c>bae_restore_from_code</c>: an id, or an error.</summary>
public sealed class RestoreResult
{
    public string? LibraryId { get; set; }
    public string? Error { get; set; }
}
