# Mobile OAuth library linking

Cloud libraries on a provider that needs OAuth (Google Drive, Dropbox,
OneDrive) require an access token to restore on a device. Desktop runs the flow
through coven's localhost callback server + the system browser; that doesn't
work in the iOS/Android sandbox (no reachable localhost server, no `open::that`
into the system browser). Mobile drives the flow itself through the native auth
session and a custom-scheme redirect.

## Flow

1. **Onboarding** decodes the restore code. When `needs_oauth` is set, it runs
   the OAuth flow before restoring; CloudKit and S3 need no token and restore
   with `oauthTokenJson: nil`.
2. `oauth_begin(provider, redirect_uri)` (bridge → `coven::oauth::
   build_authorize_request_for_provider`) returns the authorization URL + a PKCE
   verifier. No localhost port is bound and no browser is opened.
3. The app opens the URL in the native auth session
   (`ASWebAuthenticationSession` on iOS, Custom Tabs on Android) with the custom
   redirect scheme as the callback scheme, and captures the `code` from the
   redirect.
4. `oauth_complete(provider, code, verifier, redirect_uri)` exchanges the code
   for tokens and returns the token JSON.
5. `restore_from_code(code, oauthTokenJson:)` restores with the token.

coven's `authorize` (desktop) and the new `build_authorize_url` /
`build_authorize_request_for_provider` share the URL build and `exchange_code`;
only the redirect *capture* differs (localhost server vs. native auth session).

## Credentials

bae and coven ship no OAuth credentials. You register your own OAuth app's
client id + redirect URI per provider via a **gitignored** `oauth-creds.json`,
loaded at launch and registered through `set_oauth_client_creds`. The client id
is also used by coven to refresh provider tokens during sync, so the file must
be present for cloud sync of OAuth providers to keep working — not just at link
time.

File shape:

```json
{
  "google_drive": {
    "client_id": "<id>.apps.googleusercontent.com",
    "redirect_uri": "com.googleusercontent.apps.<id>:/oauth2redirect"
  }
}
```

`client_secret` is optional (installed-app clients are PKCE-only and omit it).

- **iOS**: `bae-ios/bae/bae/oauth-creds.json`. The `bae` directory source bundles
  it automatically; rerun `xcodegen generate` after adding it. The redirect's
  scheme (everything before the `:`) is the `ASWebAuthenticationSession` callback
  scheme — no URL-scheme entry in `Info.plist` is needed.
- **Android**: `bae-android/app/src/main/assets/oauth-creds.json`. The redirect
  scheme must also be declared as an `intent-filter` on the redirect-capture
  activity (see the manifest).

## Provider console setup

Create an **installed-app** OAuth client (Google: an iOS client and an Android
client; their reversed-client-id is the redirect) and add the exact
`redirect_uri` above to it. The redirect URI in `oauth-creds.json` must match the
console entry character-for-character.
