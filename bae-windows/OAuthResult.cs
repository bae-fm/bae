namespace Bae.Windows;

/// <summary>
/// The outcome of an OAuth flow (<c>bae_oauth_authorize</c>): the provider's token
/// JSON to hand to a restore, or a message describing why it failed
/// (denied, cancelled, timed out, network).
/// </summary>
internal sealed class OAuthResult
{
    public string? Token { get; set; }
    public string? Error { get; set; }
}
