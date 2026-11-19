# MacS3Form

The S3 storage form for a new library: a Location box (endpoint, region, bucket) and a Key box (access key ID, secret access key), then Cancel and Set up.

| Prop | |
| --- | --- |
| `error` | `none` (the default), or what S3 storage refused. `key` and `denied` show as a quiet notice above the form, `region` under the region field, and `bucket` and `occupied` under the bucket field. The app keeps the `key` and `denied` notices in a failed setup run's own state, which a shot of the form can't set, so those two rows get no shot. |

**In the code:** macOS: `S3SetupForm` (`bae-macos/bae/bae/Views/Settings/Library/S3SetupForm.swift`).
