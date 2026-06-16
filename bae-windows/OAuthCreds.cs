using System.IO;

namespace Bae.Windows;

/// <summary>
/// OAuth client credentials for the cloud providers that need them (Google Drive,
/// Dropbox, OneDrive), loaded from a gitignored <c>oauth-creds.json</c> placed next
/// to the executable. bae and coven ship no credentials — a build registers its own
/// OAuth app's client id (and optional secret) so coven can build authorization URLs
/// and refresh tokens. When the file is absent, cloud sign-in stays unavailable, the
/// same as the iOS/Android builds.
///
/// File shape (the same one the other platforms read; a <c>redirect_uri</c> field, if
/// present, is ignored — the desktop loopback flow builds its own 127.0.0.1 redirect):
/// <code>
/// { "google_drive": { "client_id": "&lt;id&gt;", "client_secret": null } }
/// </code>
/// </summary>
internal static class OAuthCreds
{
    /// <summary>
    /// True once a bundled oauth-creds.json registered successfully. When false,
    /// cloud sign-in is unavailable and the UI says so rather than starting a doomed
    /// flow against empty credentials.
    /// </summary>
    internal static bool Available { get; private set; }

    /// <summary>
    /// Why registration failed when a creds file was present but unusable (unreadable,
    /// or rejected by the core as malformed), for the UI to surface. Null when no file
    /// was bundled or registration succeeded.
    /// </summary>
    internal static string? RegistrationError { get; private set; }

    /// <summary>
    /// Load oauth-creds.json from the application directory and register it with the
    /// core. Call once at launch, before any OAuth flow. An absent file leaves cloud
    /// sign-in unavailable without an error (the developer didn't bundle credentials).
    /// </summary>
    internal static void Register()
    {
        // The libre build's native library exports no OAuth entry points
        // (bae_set_oauth_client_creds among them), so even reaching the
        // registration call would fault on a missing symbol. The
        // available-provider set is the gate: an S3-only build has nothing to
        // register, regardless of whether a creds file happens to be present.
        if (!NativeBae.SupportsOAuthProviders())
        {
            return;
        }

        var path = Path.Combine(AppContext.BaseDirectory, "oauth-creds.json");
        if (!File.Exists(path))
        {
            return;
        }

        // The file exists (checked above), so any read failure — an IO error, an
        // ACL denial, an unsupported path — is a real fault to surface, not the
        // legitimate absent-file skip. Catch broadly so a bad bundled file leaves
        // cloud sign-in unavailable with a reason rather than crashing launch.
        string json;
        try
        {
            json = File.ReadAllText(path);
        }
        catch (Exception e)
        {
            RegistrationError = $"couldn't read oauth-creds.json: {e.Message}";
            return;
        }

        RegistrationError = NativeBae.SetOauthClientCreds(json);
        Available = RegistrationError is null;
    }
}
