/// Normalize and validate a raw form into a wire edit: `Valid` carries the
/// savable payload, `Invalid` the reason it can't be saved. Stateless
/// type-translation wrapper around [`bae_import::RawReleaseEdit::shape`].
#[cfg(desktop)]
#[uniffi::export]
pub fn shape_release_edit(
    raw: crate::types::BridgeRawReleaseEdit,
) -> crate::types::BridgeShapeResult {
    let core_raw = raw.into_core();
    match core_raw.shape() {
        Ok(edit) => crate::types::BridgeShapeResult::Valid {
            edit: crate::types::BridgeReleaseUserEdit::from_core(edit),
        },
        Err(e) => crate::types::BridgeShapeResult::Invalid {
            reason: crate::types::BridgeValidationReason::from_core(e),
        },
    }
}

/// The localization key for a validation reason, resolved by the UI against the
/// generated `Core` string table. One exported mapping keeps every platform's
/// keys identical.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_validation_reason_key(reason: crate::types::BridgeValidationReason) -> String {
    reason.loc_key().to_string()
}
