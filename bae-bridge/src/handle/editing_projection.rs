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

/// The media and Discogs details the Add menus of `pressing` offer, by core's
/// rule. Fixtures that build a draft's read by hand call this for the record
/// the read hands beside the edit.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_pressing_offers(
    pressing: crate::types::BridgeRawPressingEdit,
) -> crate::types::BridgePressingOffers {
    crate::types::BridgePressingOffers::from_core(pressing.into_core().offers())
}

/// The catalog number the one-line summary of `pressing` shows, by core's
/// rule. Fixtures that build a draft's read by hand call this for the number
/// the read hands beside the edit.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_summary_catalog_number(
    pressing: crate::types::BridgeRawPressingEdit,
) -> Option<String> {
    pressing.into_core().summary_catalog_number()
}

/// The localization key for a validation reason, resolved by the UI against the
/// generated `Core` string table. One exported mapping keeps every platform's
/// keys identical.
#[cfg(desktop)]
#[uniffi::export]
pub fn bridge_validation_reason_key(reason: crate::types::BridgeValidationReason) -> String {
    reason.loc_key().to_string()
}
