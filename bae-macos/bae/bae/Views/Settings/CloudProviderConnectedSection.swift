import SwiftUI

struct CloudProviderConnectedSection: View {
    let config: BridgeSyncConfig
    let onDisconnect: () -> Void

    var body: some View {
        Group {
            LabeledContent("Provider") {
                Text(config.cloudProviderLabel)
            }
            if let account = config.cloudAccountDisplay {
                LabeledContent("Account") {
                    Text(account)
                        .foregroundStyle(.secondary)
                }
            }

            switch config.provider {
            case .s3(let bucket, let region, let endpoint):
                if let bucket {
                    LabeledContent("Bucket") {
                        Text(bucket)
                            .foregroundStyle(.secondary)
                    }
                }
                if let region {
                    LabeledContent("Region") {
                        Text(region)
                            .foregroundStyle(.secondary)
                    }
                }
                if let endpoint, !endpoint.isEmpty {
                    LabeledContent("Endpoint") {
                        Text(endpoint)
                            .foregroundStyle(.secondary)
                    }
                }
            case .googleDrive, .dropbox, .oneDrive, .cloudKit:
                EmptyView()
            }

            HStack {
                Spacer()
                Button("Disconnect") {
                    onDisconnect()
                }
                .foregroundStyle(.red)
            }
        }
    }
}

// MARK: - Previews

#Preview("Sync Connected - S3") {
    Form {
        Section("Sync") {
            CloudProviderConnectedSection(
                config: BridgeSyncConfig(
                    provider: .s3(
                        bucket: "my-bucket",
                        region: "us-east-1",
                        endpoint: "https://s3.example.com"
                    ),
                    cloudProviderLabel: cloudProviderLabel(provider: .s3),
                    cloudAccountDisplay: "s3://my-bucket"
                ),
                onDisconnect: {},
            )
        }
    }
    .formStyle(.grouped)
    .frame(width: 500, height: 300)
}
