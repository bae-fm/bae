import BaeKit
import Foundation
import SwiftUI

@testable import bae

// MARK: - Shot staging

extension SyncSetupWizard {
    /// Where a shot stages the wizard's S3 setup.
    enum SetupStaging {
        /// Running `step`, before its point of no return.
        case running(BridgeStorageSetupStep)
        /// Running `step`, past its point of no return.
        case finishing(BridgeStorageSetupStep)
        /// The person cancelled it while it connected.
        case cancelling
        /// The setup failed with `failure`, the form as typed.
        case failed(BridgeStorageFailure)
        /// The service refused the key, which shows at the top.
        case refusedKey(S3SetupFields.KeyRejection)
    }

    /// The S3 form's fields as a shot types them.
    private static func stagedFields() -> S3SetupFields {
        let fields = S3SetupFields()
        fields.endpoint = "https://s3.home.lan"
        fields.region = "us-east-1"
        fields.bucket = "my-bucket"
        fields.accessKey = "a1b2c3d4e5f6"
        fields.secretKey = "abcdefghijkl"
        return fields
    }

    /// The wizard on the S3 form, its setup staged at `staging` in
    /// place of core's.
    static func staged(_ staging: SetupStaging) -> some View {
        let sync = Sync(cancelStorageSetup: { .undoing })
        let (step, finishing): (BridgeStorageSetupStep, Bool) =
            switch staging {
            case .running(let step): (step, false)
            case .finishing(let step): (step, true)
            case .cancelling, .failed, .refusedKey: (.connecting, false)
            }
        let run = StorageSetupRun(onCancelled: {})
        if case .failed = staging {
        }
        else if case .refusedKey = staging {
        }
        else {
            run.start(
                StorageSetupFollowing(
                    staging: BridgeStorageSetupProgress(
                        step: step,
                        key: "core.library.storage_setup_step.connecting",
                        done: 0,
                        total: 1
                    ),
                    finishing: finishing
                ),
                operation: { _ in try await Task.neverAnswers() },
                onConnected: { _ in },
                onFailed: { _ in }
            )
        }
        if case .cancelling = staging {
            run.cancel(sync)
        }
        var wizard = SyncSetupWizard(provider: .s3, run: run, onDone: {})
        if case .failed(let failure) = staging {
            let fields = stagedFields()
            _ = fields.place(
                BridgeError.Diagnostic(
                    category: .storage(failure: failure),
                    detail: ""
                )
            )
            wizard = wizard.staging(s3: fields)
        }
        if case .refusedKey(let rejection) = staging {
            let fields = stagedFields()
            wizard = wizard.staging(
                s3: fields,
                topFailure: DisplayError(line: rejection.words)
            )
        }
        return
            wizard
            .environment(PreviewData.configStore())
            .environment(sync)
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
    }
}

extension SyncSetupWizard {
    /// Setup's last screen for `provider`, in `state`: a fixture code in
    /// a shot, never a real one.
    static func finished(
        on provider: BridgeCloudProvider,
        holding state: RestoreCodeReveal.State
    ) -> some View {
        SyncSetupWizard(
            restoreCode: RestoreCodeReveal(previewState: state),
            provider: provider,
            onDone: {}
        )
        .environment(PreviewData.configStore())
        .environment(PreviewData.previewSync(storage: nil))
        #if BAE_OAUTH_PROVIDERS
            .environment(CloudSyncSetup())
        #endif
    }
}

extension SyncSetupWizard {
    /// The wizard on iCloud's screen, before setup starts.
    static func iCloud() -> some View {
        SyncSetupWizard(
            provider: .cloudKit,
            run: StorageSetupRun(onCancelled: {}),
            onDone: {}
        )
        .environment(PreviewData.configStore())
        .environment(PreviewData.previewSync(storage: nil))
        #if BAE_OAUTH_PROVIDERS
            .environment(CloudSyncSetup())
        #endif
    }
}

extension SyncSetupWizard {
    /// The wizard as it opens, on its list of providers.
    static func choosing() -> some View {
        SyncSetupWizard(onDone: {})
            .environment(PreviewData.configStore())
            .environment(PreviewData.previewSync(storage: nil))
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
    }
}
