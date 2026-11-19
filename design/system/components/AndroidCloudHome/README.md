# AndroidCloudHome

An S3 library's cloud home in Settings on Android: a row each for the provider, endpoint, region, bucket and access key ID, then a Replace key row.

| Prop | |
| --- | --- |
| `keyId` | The access key ID this device uses. |
| `oldKey` | The key the person just replaced: a notice under the card asks them to delete it in their provider's console. |

**In the code:** Android: `StorageRows` (`bae-android/app/src/main/java/fm/bae/app/ui/settings/SyncStorageRows.kt`).
