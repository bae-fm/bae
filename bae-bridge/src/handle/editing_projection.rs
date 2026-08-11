/// Normalize + validate the editor's raw form into a wire edit. `Valid`
/// carries the savable commit payload; `Invalid` carries the reason the
/// editor disables Save. The editor calls this on every change (to gate Save
/// and show the reason) and on commit (to read the payload it passes to
/// `update_release_metadata_user_edit` / `start_import`). Stateless
/// type-translation wrapper around [`bae_core::import::RawReleaseEdit::shape`].
#[cfg(feature = "desktop")]
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

/// Map bae-core's validation error to its bridge mirror. Kept here, not as a
/// `From` in bae-core, so bae-core stays unaware of bridge types.
#[cfg(feature = "desktop")]
impl crate::types::BridgeValidationReason {
    pub(super) fn from_core(e: bae_core::import::EditValidationError) -> Self {
        use crate::types::BridgeValidationReason as R;
        use bae_core::import::EditValidationError as E;
        match e {
            E::EmptyAlbumTitle => R::EmptyAlbumTitle,
            E::NoAlbumArtist => R::NoAlbumArtist,
            E::InvalidYear => R::InvalidYear,
        }
    }
}

/// The localization key for a validation reason, resolved by the UI against the
/// generated `Core` string table. One exported mapping keeps every platform's
/// keys identical.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_validation_reason_key(reason: crate::types::BridgeValidationReason) -> String {
    reason.loc_key().to_string()
}

/// Seed the editor's raw form from a wire edit — the inverse of
/// `shape_release_edit`: joins artist lists into comma text and renders absent
/// pressing fields as empty. `track_id_prefix` supplies the editor row
/// identities the wire edit lacks. Stateless type translation around
/// [`bae_core::import::RawReleaseEdit::from_user_edit`]. Used by reset-to-source
/// to repopulate the form from the projected edit.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn raw_release_edit_from_user_edit(
    edit: crate::types::BridgeReleaseUserEdit,
    track_id_prefix: String,
) -> crate::types::BridgeRawReleaseEdit {
    let core_edit = edit.into_core();
    let raw = bae_core::import::RawReleaseEdit::from_user_edit(core_edit, &track_id_prefix);
    crate::types::BridgeRawReleaseEdit::from_core(raw)
}
