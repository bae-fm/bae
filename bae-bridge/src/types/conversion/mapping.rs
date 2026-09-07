use super::super::*;

impl BridgeFileInfo {
    fn from_core(f: &bae_core::import::folder_scanner::ScannedFile) -> Self {
        BridgeFileInfo {
            name: f.relative_path.clone(),
            size: f.size,
            dir_prefix: f.dir_prefix.clone(),
            file_name: f.file_name.clone(),
            local_path: f.path.to_string_lossy().to_string(),
            audio_format: f
                .source_audio
                .as_ref()
                .map(|audio| BridgeAudioFormat::from_core(audio.format.clone())),
        }
    }
}

impl BridgeCandidateSourceAudio {
    /// Borrowed core input: `CandidateSourceAudio` carries the folder's files by
    /// reference, so this is not `mirror_struct`'s owned copy.
    fn from_core(source_audio: bae_core::import::folder_scanner::CandidateSourceAudio<'_>) -> Self {
        let bae_core::import::folder_scanner::CandidateSourceAudio { summary, files } =
            source_audio;
        Self {
            summary: BridgeSourceAudioSummary::from_core(summary),
            files: files.into_iter().map(BridgeFileInfo::from_core).collect(),
        }
    }
}

impl BridgeCandidateFile {
    fn from_core(
        entry: bae_core::import::folder_scanner::CandidateFile,
        becomes: bae_core::import::folder_scanner::FileBecomes,
    ) -> Self {
        use bae_core::import::folder_scanner::{CandidateFile, FileRole};

        let alternatives = entry
            .role_alternatives()
            .iter()
            .copied()
            .map(BridgeFileRoleChoice::from_core)
            .collect();
        let role_choice = entry.role_choice().map(BridgeFileRoleChoice::from_core);
        let CandidateFile {
            file,
            role,
            proposed_audio: _,
        } = entry;
        // Read the file id (relative path) and disk path back off `BridgeFileInfo`
        // so the exhaustive `ScannedFile` destructure lives only in its `from_core`.
        let file = BridgeFileInfo::from_core(&file);
        let image_choice = || BridgeCoverChoice {
            selection: BridgeCoverSelection::ReleaseImage {
                file_id: file.name.clone(),
            },
            preview_source: BridgeCoverImageSource::Local {
                path: file.local_path.clone(),
            },
            thumbnail_source: BridgeCoverImageSource::Local {
                path: file.local_path.clone(),
            },
        };
        let role = match role {
            FileRole::Audio => BridgeFileRole::Audio,
            // The disc assignment is the mapping table's to show, on the group
            // header that carries the picker for it. A roles row states what
            // the sheet's slots are, which already reflects the assignment.
            FileRole::TrackSheet { sheet, .. } => BridgeFileRole::TrackSheet {
                // A derived count, not a carried field — `CueSheet` is a large
                // parse product the bridge doesn't mirror.
                track_count: sheet.playable_track_count() as u32,
            },
            FileRole::Artwork => BridgeFileRole::Artwork {
                choice: image_choice(),
            },
            FileRole::Document => BridgeFileRole::Document,
            FileRole::Other => BridgeFileRole::Other,
        };
        BridgeCandidateFile {
            file,
            role,
            becomes: BridgeFileBecomes::from_core(becomes),
            alternatives,
            role_choice,
        }
    }
}

mirror_enum! {
    BridgeFileRoleChoice = bae_core::import::folder_scanner::FileRoleChoice,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Audio, NotATrack },
}

mirror_enum! {
    BridgeFileBecomes = bae_core::import::folder_scanner::FileBecomes,
    from_core: fn,
    variants: { Slots { first, last }, NoSlots },
}

mirror_enum! {
    BridgeSheetBindingOffer = bae_core::import::folder_scanner::SheetBindingOffer,
    from_core: fn,
    variants: { Offered, RefusedCodec { codec }, RefusedTiming, RefusedUnreadable },
}

mirror_struct! {
    BridgeSheetBindingOption = bae_core::import::folder_scanner::SheetBindingOption,
    from_core: pub(crate) fn,
    fields: { file_id, offer: (BridgeSheetBindingOffer) },
}

impl BridgeCandidateFiles {
    pub(crate) fn from_core(files: bae_core::import::folder_scanner::CategorizedFiles) -> Self {
        // Derived from the whole set before it is taken apart: which slots a
        // file backs is a fact about the folder, not about any one file.
        let becomes = files.becomes();
        let source_audio = files
            .source_audio()
            .map(BridgeCandidateSourceAudio::from_core);
        let file_tags_identity = files.file_tags_identity();
        let bae_core::import::folder_scanner::CategorizedFiles { files } = files;
        BridgeCandidateFiles {
            file_tags_identity,
            files: files
                .into_iter()
                .zip(becomes)
                .map(|(entry, becomes)| BridgeCandidateFile::from_core(entry, becomes))
                .collect(),
            source_audio,
        }
    }
}

mirror_struct! {
    BridgePressingEdit = bae_core::import::PressingEdit,
    from_core: pub(super) fn,
    into_core: pub(super) fn,
    fields: { year, format, label, catalog_number, country, barcode },
}

mirror_enum! {
    BridgeAudioFile = bae_core::import::AudioFile,
    from_core: pub(crate) fn,
    into_core: pub(super) fn,
    variants: {
        Standalone { file_id },
        SheetSlice { file_id, sheet_id, index },
    },
}

mirror_enum! {
    BridgeSlotReconciliation = bae_core::import::SlotReconciliation,
    from_core: fn,
    into_core: fn,
    variants: {
        Agrees { count },
        MoreFiles { files, tracks },
        MoreTracks { files, tracks },
    },
}

mirror_enum! {
    BridgeSheetDisc = bae_core::import::folder_scanner::SheetDisc,
    from_core: fn,
    into_core: pub(crate) fn,
    variants: { Disc { number }, Ignored },
}

mirror_enum! {
    BridgeMappingRole = bae_core::import::MappingRole,
    from_core: fn,
    into_core: fn,
    variants: { Audio, Document, Other },
}

impl BridgeMappingFile {
    fn from_core(file: bae_core::import::MappingFile) -> Self {
        let bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path,
            preview_target,
            duration_ms,
            audio_format,
            role,
            alternatives,
            role_choice,
        } = file;
        BridgeMappingFile {
            role: BridgeMappingRole::from_core(role),
            local_path: path.to_string_lossy().to_string(),
            preview_target: preview_target.map(BridgePreviewTarget::from_core),
            file_id,
            name,
            size,
            duration_ms,
            audio_format: audio_format.map(BridgeAudioFormat::from_core),
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::from_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::from_core),
        }
    }

    fn into_core(self) -> bae_core::import::MappingFile {
        let BridgeMappingFile {
            file_id,
            name,
            size,
            local_path,
            preview_target,
            duration_ms,
            audio_format,
            role,
            alternatives,
            role_choice,
        } = self;
        bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
            preview_target: preview_target.map(BridgePreviewTarget::into_core),
            duration_ms,
            audio_format: audio_format.map(BridgeAudioFormat::into_core),
            role: role.into_core(),
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::into_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::into_core),
        }
    }
}

impl BridgeMappingEntry {
    fn from_core(entry: bae_core::import::MappingEntry) -> Self {
        let bae_core::import::MappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_path,
            preview_target,
            audio_format,
        } = entry;
        BridgeMappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_local_path: container_path.to_string_lossy().to_string(),
            preview_target: BridgePreviewTarget::from_core(preview_target),
            audio_format: BridgeAudioFormat::from_core(audio_format),
        }
    }

    fn into_core(self) -> bae_core::import::MappingEntry {
        let BridgeMappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_local_path,
            preview_target,
            audio_format,
        } = self;
        bae_core::import::MappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_path: std::path::PathBuf::from(container_local_path),
            preview_target: preview_target.into_core(),
            audio_format: audio_format.into_core(),
        }
    }
}

mirror_enum! {
    BridgeMappingSource = bae_core::import::MappingSource,
    from_core: fn,
    into_core: fn,
    variants: {
        File(file: (BridgeMappingFile)),
        SheetEntry(entry: (BridgeMappingEntry)),
        Missing,
    },
}

mirror_enum! {
    BridgeMappingBecomes = bae_core::import::MappingBecomes,
    from_core: fn,
    into_core: fn,
    variants: {
        Track { track: (BridgeRawTrackEdit), position, named_by_source },
        AwaitingPick,
    },
}

mirror_struct! {
    BridgeTrackMapping = bae_core::import::TrackMapping,
    from_core: fn,
    into_core: fn,
    fields: {
        source: (BridgeMappingSource),
        becomes: (BridgeMappingBecomes),
        duration_ms,
    },
}

mirror_struct! {
    BridgeMappingContainer = bae_core::import::MappingContainer,
    from_core: fn,
    into_core: fn,
    fields: { file_id, name, size, audio_format: (BridgeAudioFormat) },
}

impl BridgeSheetGroup {
    fn from_core(sheet: bae_core::import::SheetGroup) -> Self {
        let bae_core::import::SheetGroup {
            sheet_id,
            name,
            size,
            path,
            bound,
            assignment,
            disc_options,
        } = sheet;
        BridgeSheetGroup {
            sheet_id,
            name,
            size,
            local_path: path.to_string_lossy().into_owned(),
            bound: BridgeSheetBound::from_core(bound),
            assignment: BridgeSheetDisc::from_core(assignment),
            disc_options,
        }
    }

    fn into_core(self) -> bae_core::import::SheetGroup {
        let BridgeSheetGroup {
            sheet_id,
            name,
            size,
            local_path,
            bound,
            assignment,
            disc_options,
        } = self;
        bae_core::import::SheetGroup {
            sheet_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
            bound: bound.into_core(),
            assignment: assignment.into_core(),
            disc_options,
        }
    }
}

mirror_enum! {
    BridgeSheetBound = bae_core::import::SheetBound,
    from_core: fn,
    into_core: fn,
    variants: {
        Describes(container: (BridgeMappingContainer)),
        DescribesFiles,
        Unresolved { requested },
        RefusedCodec { codec },
    },
}

impl BridgeMappingImage {
    fn from_core(image: bae_core::import::MappingImage) -> Self {
        let bae_core::import::MappingImage {
            file_id,
            name,
            size,
            path,
        } = image;
        BridgeMappingImage {
            file_id,
            name,
            size,
            local_path: path.to_string_lossy().to_string(),
        }
    }

    fn into_core(self) -> bae_core::import::MappingImage {
        let BridgeMappingImage {
            file_id,
            name,
            size,
            local_path,
        } = self;
        bae_core::import::MappingImage {
            file_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
        }
    }
}

mirror_enum! {
    BridgeMappingTrackSectionContent = bae_core::import::MappingTrackSectionContent,
    from_core: fn,
    into_core: fn,
    variants: {
        Tracks(mappings: (each BridgeTrackMapping)),
        Sheet {
            sheet: (BridgeSheetGroup),
            entries: (each BridgeTrackMapping),
        },
    },
}

impl BridgeMappingTrackSection {
    fn from_core(section: bae_core::import::MappingTrackSection) -> Self {
        let bae_core::import::MappingTrackSection { side, content } = section;
        let side = BridgeTrackSide::from_core(side);
        Self {
            header_key: side.header_key().map(str::to_string),
            side,
            content: BridgeMappingTrackSectionContent::from_core(content),
        }
    }

    fn into_core(self) -> bae_core::import::MappingTrackSection {
        bae_core::import::MappingTrackSection {
            side: self.side.into_core(),
            content: self.content.into_core(),
        }
    }
}

mirror_enum! {
    BridgeMappingFileRow = bae_core::import::MappingFileRow,
    from_core: fn,
    into_core: fn,
    variants: {
        File(file: (BridgeMappingFile)),
        Sheet(sheet: (BridgeSheetGroup)),
    },
}

mirror_struct! {
    BridgeMappingTable = bae_core::import::MappingTable,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        images: (each BridgeMappingImage),
        track_sections: (each BridgeMappingTrackSection),
        files: (each BridgeMappingFileRow),
        reconciliation: (opt BridgeSlotReconciliation),
    },
}
