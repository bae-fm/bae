# MacCloudHome

A library's cloud home in the Sync settings: the Cloud home heading over the rows that say where its data is, then what this Mac uses to reach it.

| Prop | |
| --- | --- |
| `provider` | `s3` (the default), `dropbox`, `onedrive` or `icloud`. Each provider has its rows: S3 the endpoint, region and bucket; Dropbox and OneDrive the account and the folder when bae knows them; iCloud the provider alone. |
| `endpoint`, `region`, `bucket` | S3's rows. |
| `account`, `folder` | Dropbox's or OneDrive's rows. With a folder, the box ends in Open in browser, which opens the folder's page at the provider. |
| `keyId` | On S3, the access key ID this Mac uses, with Replace key…. |
| `oldKey` | On S3, the key the person just replaced: a notice under the box asks them to delete it in their provider's console. |
| `bare` | The provider's rows alone, with no heading, key row or Open in browser, for a box that holds other rows too: the welcome window's library details put them under the status. |

**In the code:** macOS: `MacCloudHome` (`Views/Settings/Library/MacCloudHome.swift`), in the Cloud home section of `LibrarySettingsTab`: BaeKit's `CloudProviderConnectedSection` for the rows, the Access key ID row, `OpenFolderInBrowserRow` and `S3KeyLineNotice`. `WelcomeLibraryDetails` draws it `bare`, which is `CloudProviderConnectedSection` alone.
