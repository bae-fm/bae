use super::*;

/// The form a library release's editor holds: what the editor opened on, the
/// form with each change the person made, the catalog release the person
/// applied and the album they picked. The app holds it and core keeps none:
/// each change returns a new form, the editor draws the form's values
/// ([`Self::edit`]), the editor's draft subscription reads the form the app
/// sets on it, and a save writes the form the app sends.
#[derive(uniffi::Object)]
pub struct BridgeReleaseEditForm {
    inner: bae_import::ReleaseEditForm,
}

impl BridgeReleaseEditForm {
    fn of(inner: bae_import::ReleaseEditForm) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self { inner })
    }

    /// The form as core reads and saves it.
    pub(crate) fn core(&self) -> &bae_import::ReleaseEditForm {
        &self.inner
    }

    /// This form with `change` made to a copy of it.
    fn changed(
        &self,
        change: impl FnOnce(&mut bae_import::ReleaseEditForm),
    ) -> std::sync::Arc<Self> {
        let mut inner = self.inner.clone();
        change(&mut inner);
        Self::of(inner)
    }
}

#[uniffi::export]
impl BridgeReleaseEditForm {
    /// The release the form edits.
    pub fn release_id(&self) -> String {
        self.inner.release_id.clone()
    }

    /// The form's values as the editor draws them: each field, label,
    /// barcode, pressing fact, album artist and track row as the form holds
    /// it now.
    pub fn edit(&self) -> crate::types::BridgeRawReleaseEdit {
        crate::types::BridgeRawReleaseEdit::from_core(self.inner.edit.clone())
    }

    /// The form with one album-level text field as the person left it.
    pub fn with_field(
        &self,
        field: crate::types::BridgeCandidateEditField,
        value: String,
    ) -> std::sync::Arc<Self> {
        let edit = bae_import::DraftFieldEdit::Text { field: field.into_core(), value };
        self.changed(|form| form.edit.set_field(edit))
    }

    /// The form with its label rows as the person left them; a label that
    /// keeps the name it had keeps its catalog ids.
    pub fn with_labels(
        &self,
        labels: Vec<crate::types::BridgeRawLabelEdit>,
    ) -> std::sync::Arc<Self> {
        let edit = bae_import::DraftFieldEdit::Labels(
            labels.into_iter().map(crate::types::BridgeRawLabelEdit::into_core).collect(),
        );
        self.changed(|form| form.edit.set_field(edit))
    }

    /// The form with its barcode rows as the person left them.
    pub fn with_barcodes(&self, barcodes: Vec<String>) -> std::sync::Arc<Self> {
        let edit = bae_import::DraftFieldEdit::Barcodes(barcodes);
        self.changed(|form| form.edit.set_field(edit))
    }

    /// The form with one choice of what the pressing is.
    pub fn with_pressing_fact(
        &self,
        fact: crate::types::BridgePressingFactEdit,
    ) -> std::sync::Arc<Self> {
        let edit = bae_import::DraftFieldEdit::PressingFact(fact.into_core());
        self.changed(|form| form.edit.set_field(edit))
    }

    /// The form with `assignments` as its ordered album credits.
    pub fn with_album_artists(
        &self,
        assignments: Vec<crate::types::BridgeCredited>,
    ) -> std::sync::Arc<Self> {
        let assignments: Vec<_> =
            assignments.into_iter().map(crate::types::BridgeCredited::into_core).collect();
        self.changed(|form| form.edit.set_album_artists(assignments))
    }

    /// The form with one track row's title and artists as the person left
    /// them, by the row's id. Refused for a row the form does not hold.
    pub fn with_track(
        &self,
        track: crate::types::BridgeRawTrackEdit,
    ) -> Result<std::sync::Arc<Self>, BridgeError> {
        let track = track.into_core();
        let id = track.id.clone();
        let mut inner = self.inner.clone();
        if inner.edit.set_track(track) {
            Ok(Self::of(inner))
        } else {
            Err(bae_library::LibraryError::EditRowMissing(id).into())
        }
    }

    /// The form with `pick` as the album a save files the release under: a
    /// library album, a new one, or the album its records name.
    pub fn with_album(&self, pick: crate::types::BridgeAlbumPick) -> std::sync::Arc<Self> {
        let pick = pick.into_core();
        self.changed(|form| form.album_pick = pick)
    }
}

forward! {
    #[cfg(desktop)]
    async this => {
        /// The form an editor opens with on a library release: its values
        /// as the library holds them now, with nothing edited, applied or
        /// picked. Cancel opens the form again.
        fn open_release_edit(release_id: String) -> std::sync::Arc<BridgeReleaseEditForm> {
            Ok(BridgeReleaseEditForm::of(this.services.open_release_edit(&release_id).await?))
        }

        /// `form` with the catalog release `link` picks applied: every value
        /// it gives replaces the form's, held in the form until a save. When
        /// the app shows the apply's step, core writes each network read it
        /// waits on to `apply`'s row while the read runs.
        fn apply_release_edit_catalog(
            form: std::sync::Arc<BridgeReleaseEditForm>,
            link: crate::types::BridgePressingLink,
            apply: Option<std::sync::Arc<crate::BridgeCatalogApply>>,
        ) -> std::sync::Arc<BridgeReleaseEditForm> {
            let steps = crate::screens::catalog_apply_steps(apply.as_deref());
            Ok(BridgeReleaseEditForm::of(
                this.services
                    .apply_release_edit_catalog(form.inner.clone(), &link.into_core(), &steps)
                    .await?,
            ))
        }

        /// Save `form` into the album it files the release under: every
        /// value, in one write. Returns the album the release is on after
        /// the save and the form opened on what the save wrote. Core refuses
        /// the save and writes nothing when the form holds a value core
        /// can't save, or when the release changed after the form opened on
        /// it (`BridgeErrorCategory::ReleaseChangedSinceOpened`).
        fn save_release_edit(
            form: std::sync::Arc<BridgeReleaseEditForm>,
        ) -> crate::types::BridgeReleaseEditSaved {
            let saved = this.services.save_release_edit(form.inner.clone(), bae_library::PickedAlbumGone::Unpick).await?;
            let album_id = this.services.get_album_id_for_release(&saved.release_id).await?;
            Ok(crate::types::BridgeReleaseEditSaved { album_id, form: BridgeReleaseEditForm::of(saved) })
        }
    }
}

#[uniffi::export]
impl AppHandle {
    /// A release editor's draft, showing nothing until `set_form` names the
    /// form the editor holds.
    pub fn subscribe_release_edit_draft(
        &self,
    ) -> std::sync::Arc<crate::ReleaseEditDraftSubscription> {
        crate::screens::release_edit_draft(
            self.services.subscribe_release_edit_draft(),
            self.runtime.clone(),
        )
    }
}
