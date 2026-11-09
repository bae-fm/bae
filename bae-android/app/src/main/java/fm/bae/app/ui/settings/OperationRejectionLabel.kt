package fm.bae.app.ui.settings

import fm.bae.app.R
import uniffi.bae_bridge.BridgeOperationRejection

/** Why the library refused a change this device asked for, as a sentence. */
internal fun BridgeOperationRejection.labelRes(): Int =
    when (this) {
        BridgeOperationRejection.STORE_OWNER -> R.string.operation_rejection_store_owner
        BridgeOperationRejection.PERSON_GONE -> R.string.operation_rejection_person_gone
        BridgeOperationRejection.DEVICE_GONE -> R.string.operation_rejection_device_gone
        BridgeOperationRejection.CIRCLE_GONE -> R.string.operation_rejection_circle_gone
        BridgeOperationRejection.NO_ADMIN_LEFT -> R.string.operation_rejection_no_admin_left
        BridgeOperationRejection.KEYS_ALREADY_USED -> R.string.operation_rejection_keys_already_used
        BridgeOperationRejection.MEMBER_KEYS_CHANGED -> R.string.operation_rejection_member_keys_changed
        BridgeOperationRejection.NOT_ALLOWED -> R.string.operation_rejection_not_allowed
        BridgeOperationRejection.ADMISSION_TOO_LARGE -> R.string.operation_rejection_admission_too_large
    }
