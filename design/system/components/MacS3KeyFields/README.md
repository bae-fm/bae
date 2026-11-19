# MacS3KeyFields

The two fields of an S3 access key, Access key ID and Secret access key, as Replace key… and joining an S3 library ask for them.

| Prop | |
| --- | --- |
| `keyId`, `secret` | What the person typed. The secret shows as dots. |
| `focus` | `secret` (the default) or `id`: the field that has focus. The app focuses the ID field only while it is empty, so the ID-focused row with a typed ID gets no shot. |
| `refused` | S3 storage turned the key down: a quiet notice above the fields says so. |

**In the code:** macOS: `S3KeyFields` (`bae-macos/bae/bae/Views/Components/S3KeyFields.swift`).
