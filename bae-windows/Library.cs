namespace Bae.Windows;

/// <summary>A library discovered by the FFI's <c>bae_libraries</c>.</summary>
public sealed class Library
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public bool IsActive { get; set; }
}
