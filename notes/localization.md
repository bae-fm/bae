# Localization

bae has one shared localization catalog and one native chrome catalog per UI.
The split follows ownership: shared Rust/bridge meanings live in the shared
catalog; labels owned by one platform's UI live in that platform's native
catalog.

## Shared catalog

`bae-bridge/loc/catalog.toml` is the shared catalog. It contains strings whose
meaning is selected by Rust or by a bridge type that every platform receives.
The catalog ids use two namespaces:

- `core.*` for meanings owned by Rust or the bridge.
- `ui.*` for shared UI text that is intentionally rendered through the same
  generated catalog.

Examples:

- `core.import.validation.empty_album_title` is selected when Rust rejects an
  edit because the album title field is empty.
- `core.error.not_found.release` is selected from a typed "release not found"
  error. Rust supplies the entity kind; the UI renders the line.
- `core.import.prepare.parsing_metadata` is selected from an import progress
  enum. Rust decides the step; the UI renders the step text.
- `core.transfer.files` is selected by storage-transfer UI code, but its
  arguments (`file_no`, `total`) are part of the shared transfer shape.

Each catalog value is an ICU MessageFormat 1 string. Arguments are declared in
the `args` table so `loc-gen check` can verify that every placeholder is real
and every declared argument is used.

Example:

```toml
[messages."core.transfer.files"]
args = { file_no = "Int", total = "Int" }
value = "{file_no} of {total} files"
```

The locale does not cross the bridge. Rust sends typed data, raw numbers, and
stable catalog keys. The platform resolves the key and formats the arguments for
the active locale.

Example flow:

1. Rust reports `BridgeInvalidReason::CorruptAudioFile { path }`.
1. The bridge exposes `bridge_invalid_reason_key(reason)`.
1. Swift resolves that key in the generated `Core` string table.
1. Swift inserts `path` into the localized format string.

## Generated shared resources

`bae-loc` validates `catalog.toml` and emits each platform's generated `Core`
resources:

- Apple: `Core.xcstrings`
- Android: `core_strings.xml`
- Windows: `Core.resw`

These generated shared resources are not committed. The repo commits the
catalog and regenerates the platform resources during the relevant build.

Apple is the only target that changes the message structure during generation:
`bae-loc` converts ICU MessageFormat placeholders into Apple string-catalog
format specifiers and converts whole-message plurals into string-catalog plural
variations.

Android and Windows store the ICU MessageFormat value verbatim in their resource
files. Their runtime localization helpers parse and format the message for the
active locale.

Example:

```toml
[messages."core.outbox.pending_deletes"]
args = { count = "Int" }
value = "{count, plural, one {# pending delete} other {# pending deletes}}"
```

Apple emits a plural variation in `Core.xcstrings`. Android and Windows store
the same MessageFormat string and let their runtime formatters choose the plural
case.

## Platform chrome catalogs

Each UI also has native platform chrome strings. These are committed and used by
the platform resource system directly:

- macOS: `bae-macos/bae/bae/Localizable.xcstrings`
- iOS: `bae-ios/bae/bae/Localizable.xcstrings`
- Android: `bae-android/app/src/main/res/values/strings.xml` plus translated
  `values-*` directories
- Windows: `bae-windows/Strings/<locale>/Resources.resw`

Chrome strings are labels, titles, buttons, placeholders, menu items, and screen
text whose meaning belongs to that UI rather than to Rust.

Examples:

- A macOS menu item such as "Add Folder to Library..." belongs in macOS chrome.
- An Android button such as "Connect" belongs in Android chrome.
- A Windows XAML label such as `settings.title.Text` belongs in Windows chrome.
- An iOS screen title such as "Albums" belongs in iOS chrome.

The same English words can belong in different places depending on who decides
the meaning. If Rust decides that a storage action is "pin", the localized
action label belongs in `catalog.toml`. If a platform has a button whose action
is attached to one screen, that button label belongs in that platform's chrome
catalog.

## Boundary examples

`catalog.toml` is the right place when the UI is rendering a typed fact from
Rust:

- Import validation reason: `empty album title`, `invalid year`.
- Playback failure reason: sync is disconnected, upload is pending.
- Error category: database, config, import, export.
- Transfer action: pin, unpin, manage, unmanage.
- Shared counts or progress lines whose arguments come from bridge types.

Platform chrome is the right place when the platform chooses the string as part
of its own layout or workflow:

- Navigation labels.
- Button labels attached to one platform screen.
- Form placeholders.
- Section headers.
- XAML `x:Uid` labels.
- SwiftUI `Text(...)` literals that are not selected by Rust state.

## Checks

`loc-gen check` validates the shared catalog's MessageFormat syntax, namespaces,
argument declarations, and placeholder usage.

`bae-bridge` has tests that keep `core.*` keys synchronized with bridge key
functions. A key produced by Rust must exist in `catalog.toml`, and a `core.*`
catalog entry must be produced by a bridge key function or listed as a direct UI
key.

`scripts/loc-chrome-orphans.py` scans platform chrome catalogs for unreferenced
keys. Runtime-built keys are listed in `scripts/loc-orphans-allowlist.txt`.
