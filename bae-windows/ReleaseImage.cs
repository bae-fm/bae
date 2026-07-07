namespace Bae.Windows;

/// <summary>
/// One of a release's local image files offered as a cover-art choice,
/// deserialized from the generated bridge's release-image JSON. The picker
/// loads the thumbnail from the generated bridge with a
/// <c>releaseFile</c> source built from <see cref="Id"/>, and passes
/// <see cref="Id"/> back to the bridge when the user selects it.
/// </summary>
public sealed class ReleaseImage
{
    public string Id { get; set; } = string.Empty;
    public string OriginalFilename { get; set; } = string.Empty;
}
