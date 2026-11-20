# MacInviteFields

The invite form's fields: the email of the person's account on Dropbox, OneDrive or iCloud, or on S3 the key they join with, then their role and how long the invite lasts. It draws the boxes alone: the Invite a person sheet (`MacSheet`) holds it, with Use my key instead and the sheet's buttons in its `MacSheetFooter`.

| Prop | |
| --- | --- |
| `storage` | `s3` (the default), `dropbox`, `onedrive` or `icloud`. |
| `keyOwner` | On S3: `theirs` (the default), their own key in two fields, or `mine`, a note that they use your key. |

Each field is named by its row's label, as VoiceOver reads it: “Dropbox email”, “Microsoft account email”, “Apple Account email”, “Access key ID”, “Secret access key”.

**In the code:** macOS: `MacInviteFields` (`bae-macos/bae/bae/Views/Settings/Library/MacInviteFields.swift`), in `InvitePersonSheet`.
