use super::super::*;

#[cfg(feature = "desktop")]
impl BridgeFileInfo {
    fn from_core(f: bae_core::import::folder_scanner::ScannedFile) -> Self {
        let bae_core::import::folder_scanner::ScannedFile {
            path,
            relative_path,
            size,
            modified_at_ns: _,
            dir_prefix,
            file_name,
            source_audio,
        } = f;
        BridgeFileInfo {
            name: relative_path,
            size,
            dir_prefix,
            file_name,
            local_path: path.to_string_lossy().to_string(),
            audio_format: source_audio.map(|audio| BridgeAudioFormat::from_core(audio.format)),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCandidateFile {
    fn from_core(
        entry: bae_core::import::folder_scanner::CandidateFile,
        becomes: bae_core::import::folder_scanner::FileBecomes,
    ) -> Self {
        use bae_core::import::folder_scanner::{CandidateFile, FileRole, SheetBinding};

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
        let file = BridgeFileInfo::from_core(file);
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
            FileRole::TrackSheet {
                sheet,
                binding,
                disc: _,
            } => BridgeFileRole::TrackSheet {
                binding: match binding {
                    SheetBinding::Describes { file_id } => {
                        BridgeSheetBinding::Describes { file_id }
                    }
                    // Derived from the parsed sheet, like `track_count` below:
                    // the directive's text is what the pane shows a user whose
                    // sheet found nothing, and the bridge doesn't mirror the
                    // whole parse to carry it.
                    SheetBinding::Unresolved => BridgeSheetBinding::Unresolved {
                        requested: sheet
                            .audio_file_references()
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                    SheetBinding::RefusedCodec { file_id, codec } => {
                        BridgeSheetBinding::RefusedCodec { file_id, codec }
                    }
                },
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

#[cfg(feature = "desktop")]
impl BridgeFileRoleChoice {
    pub(crate) fn from_core(choice: bae_core::import::folder_scanner::FileRoleChoice) -> Self {
        use bae_core::import::folder_scanner::FileRoleChoice;
        match choice {
            FileRoleChoice::Audio => Self::Audio,
            FileRoleChoice::NotATrack => Self::NotATrack,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::folder_scanner::FileRoleChoice {
        use bae_core::import::folder_scanner::FileRoleChoice;
        match self {
            Self::Audio => FileRoleChoice::Audio,
            Self::NotATrack => FileRoleChoice::NotATrack,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeFileBecomes {
    fn from_core(becomes: bae_core::import::folder_scanner::FileBecomes) -> Self {
        use bae_core::import::folder_scanner::FileBecomes;
        match becomes {
            FileBecomes::Slots { first, last } => Self::Slots { first, last },
            FileBecomes::NoSlots => Self::NoSlots,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCollapsedDirectory {
    fn from_core(directory: bae_core::import::folder_scanner::CollapsedDirectory) -> Self {
        use bae_core::import::folder_scanner::{CollapsedDirectory, FileRowKind};
        let CollapsedDirectory {
            dir_prefix,
            kind,
            count,
            total_size,
        } = directory;
        BridgeCollapsedDirectory {
            dir_prefix,
            kind: match kind {
                FileRowKind::Document => BridgeFileRowKind::Document,
                FileRowKind::Other => BridgeFileRowKind::Other,
            },
            count,
            total_size,
        }
    }

    fn into_core(self) -> bae_core::import::folder_scanner::CollapsedDirectory {
        use bae_core::import::folder_scanner::{CollapsedDirectory, FileRowKind};
        let BridgeCollapsedDirectory {
            dir_prefix,
            kind,
            count,
            total_size,
        } = self;
        CollapsedDirectory {
            dir_prefix,
            kind: match kind {
                BridgeFileRowKind::Document => FileRowKind::Document,
                BridgeFileRowKind::Other => FileRowKind::Other,
            },
            count,
            total_size,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetBindingOption {
    pub(crate) fn from_core(option: bae_core::import::folder_scanner::SheetBindingOption) -> Self {
        use bae_core::import::folder_scanner::{SheetBindingOffer, SheetBindingOption};

        let SheetBindingOption { file_id, offer } = option;
        BridgeSheetBindingOption {
            file_id,
            offer: match offer {
                SheetBindingOffer::Offered => BridgeSheetBindingOffer::Offered,
                SheetBindingOffer::RefusedCodec { codec } => {
                    BridgeSheetBindingOffer::RefusedCodec { codec }
                }
                SheetBindingOffer::RefusedTiming => BridgeSheetBindingOffer::RefusedTiming,
                SheetBindingOffer::RefusedUnreadable => BridgeSheetBindingOffer::RefusedUnreadable,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCandidateFiles {
    pub(crate) fn from_core(files: bae_core::import::folder_scanner::CategorizedFiles) -> Self {
        // Both derived from the whole set before it is taken apart: which slots
        // a file backs and which directories collapse are facts about the
        // folder, not about any one file.
        let becomes = files.becomes();
        let collapsed_directories = files
            .collapsed_directories()
            .into_iter()
            .map(BridgeCollapsedDirectory::from_core)
            .collect();
        let source_audio = files
            .source_audio_summary()
            .map(BridgeSourceAudioSummary::from_core);
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
            collapsed_directories,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgePressingEdit {
    pub(super) fn from_core(p: bae_core::import::PressingEdit) -> Self {
        let bae_core::import::PressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = p;
        Self {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::PressingEdit {
        let BridgePressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = self;
        bae_core::import::PressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeAudioFile {
    pub(crate) fn from_core(file: bae_core::import::AudioFile) -> Self {
        match file {
            bae_core::import::AudioFile::Standalone { file_id } => Self::Standalone { file_id },
            bae_core::import::AudioFile::SheetSlice {
                file_id,
                sheet_id,
                index,
            } => Self::SheetSlice {
                file_id,
                sheet_id,
                index,
            },
        }
    }

    pub(super) fn into_core(self) -> bae_core::import::AudioFile {
        match self {
            Self::Standalone { file_id } => bae_core::import::AudioFile::Standalone { file_id },
            Self::SheetSlice {
                file_id,
                sheet_id,
                index,
            } => bae_core::import::AudioFile::SheetSlice {
                file_id,
                sheet_id,
                index,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSlotReconciliation {
    fn from_core(reconciliation: bae_core::import::SlotReconciliation) -> Self {
        use bae_core::import::SlotReconciliation;
        match reconciliation {
            SlotReconciliation::Agrees { count } => Self::Agrees { count },
            SlotReconciliation::MoreFiles { files, tracks } => Self::MoreFiles { files, tracks },
            SlotReconciliation::MoreTracks { files, tracks } => Self::MoreTracks { files, tracks },
        }
    }

    fn into_core(self) -> bae_core::import::SlotReconciliation {
        use bae_core::import::SlotReconciliation;
        match self {
            Self::Agrees { count } => SlotReconciliation::Agrees { count },
            Self::MoreFiles { files, tracks } => SlotReconciliation::MoreFiles { files, tracks },
            Self::MoreTracks { files, tracks } => SlotReconciliation::MoreTracks { files, tracks },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetDisc {
    fn from_core(disc: bae_core::import::folder_scanner::SheetDisc) -> Self {
        use bae_core::import::folder_scanner::SheetDisc;
        match disc {
            SheetDisc::Disc { number } => Self::Disc { number },
            SheetDisc::Ignored => Self::Ignored,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::folder_scanner::SheetDisc {
        use bae_core::import::folder_scanner::SheetDisc;
        match self {
            Self::Disc { number } => SheetDisc::Disc { number },
            Self::Ignored => SheetDisc::Ignored,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingRole {
    fn from_core(role: bae_core::import::MappingRole) -> Self {
        use bae_core::import::MappingRole;
        match role {
            MappingRole::Audio => Self::Audio,
            MappingRole::Document => Self::Document,
            MappingRole::Other => Self::Other,
        }
    }

    fn into_core(self) -> bae_core::import::MappingRole {
        use bae_core::import::MappingRole;
        match self {
            Self::Audio => MappingRole::Audio,
            Self::Document => MappingRole::Document,
            Self::Other => MappingRole::Other,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingFile {
    fn from_core(file: bae_core::import::MappingFile) -> Self {
        let bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path,
            duration_ms,
            audio_format,
            role,
            alternatives,
            role_choice,
        } = file;
        BridgeMappingFile {
            role: BridgeMappingRole::from_core(role),
            local_path: path.to_string_lossy().to_string(),
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
            duration_ms,
            audio_format: audio_format.map(|format| bae_core::album_detail::AudioFormat {
                codec: format.codec,
                sample_rate_hz: format.sample_rate_hz,
                bits_per_sample: format.bits_per_sample,
                bitrate_kbps: format.bitrate_kbps,
                channels: format.channels,
            }),
            role: role.into_core(),
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::into_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
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
            audio_format: bae_core::album_detail::AudioFormat {
                codec: audio_format.codec,
                sample_rate_hz: audio_format.sample_rate_hz,
                bits_per_sample: audio_format.bits_per_sample,
                bitrate_kbps: audio_format.bitrate_kbps,
                channels: audio_format.channels,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingSource {
    fn from_core(source: bae_core::import::MappingSource) -> Self {
        use bae_core::import::MappingSource;
        match source {
            MappingSource::File(file) => Self::File {
                file: BridgeMappingFile::from_core(file),
            },
            MappingSource::SheetEntry(entry) => Self::SheetEntry {
                entry: BridgeMappingEntry::from_core(entry),
            },
            MappingSource::Missing => Self::Missing,
        }
    }

    fn into_core(self) -> bae_core::import::MappingSource {
        use bae_core::import::MappingSource;
        match self {
            Self::File { file } => MappingSource::File(file.into_core()),
            Self::SheetEntry { entry } => MappingSource::SheetEntry(entry.into_core()),
            Self::Missing => MappingSource::Missing,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingBecomes {
    fn from_core(becomes: bae_core::import::MappingBecomes) -> Self {
        use bae_core::import::MappingBecomes;
        match becomes {
            MappingBecomes::Track {
                track,
                source_position,
            } => Self::Track {
                track: BridgeRawTrackEdit::from_core(track),
                source_position,
            },
            MappingBecomes::Kept => Self::Kept,
            MappingBecomes::AwaitingPick => Self::AwaitingPick,
        }
    }

    fn into_core(self) -> bae_core::import::MappingBecomes {
        use bae_core::import::MappingBecomes;
        match self {
            Self::Track {
                track,
                source_position,
            } => MappingBecomes::Track {
                track: track.into_core(),
                source_position,
            },
            Self::Kept => MappingBecomes::Kept,
            Self::AwaitingPick => MappingBecomes::AwaitingPick,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingUnit {
    fn from_core(unit: bae_core::import::MappingUnit) -> Self {
        let bae_core::import::MappingUnit {
            source,
            becomes,
            duration_ms,
        } = unit;
        BridgeMappingUnit {
            source: BridgeMappingSource::from_core(source),
            becomes: BridgeMappingBecomes::from_core(becomes),
            duration_ms,
        }
    }

    fn into_core(self) -> bae_core::import::MappingUnit {
        let BridgeMappingUnit {
            source,
            becomes,
            duration_ms,
        } = self;
        bae_core::import::MappingUnit {
            source: source.into_core(),
            becomes: becomes.into_core(),
            duration_ms,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingContainer {
    fn from_core(container: bae_core::import::MappingContainer) -> Self {
        let bae_core::import::MappingContainer {
            file_id,
            name,
            size,
            audio_format,
        } = container;
        BridgeMappingContainer {
            file_id,
            name,
            size,
            audio_format: BridgeAudioFormat::from_core(audio_format),
        }
    }

    fn into_core(self) -> bae_core::import::MappingContainer {
        let BridgeMappingContainer {
            file_id,
            name,
            size,
            audio_format,
        } = self;
        bae_core::import::MappingContainer {
            file_id,
            name,
            size,
            audio_format: bae_core::album_detail::AudioFormat {
                codec: audio_format.codec,
                sample_rate_hz: audio_format.sample_rate_hz,
                bits_per_sample: audio_format.bits_per_sample,
                bitrate_kbps: audio_format.bitrate_kbps,
                channels: audio_format.channels,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetGroup {
    fn from_core(sheet: bae_core::import::SheetGroup) -> Self {
        let bae_core::import::SheetGroup {
            sheet_id,
            name,
            path,
            bound,
            assignment,
            disc_options,
        } = sheet;
        BridgeSheetGroup {
            sheet_id,
            name,
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
            local_path,
            bound,
            assignment,
            disc_options,
        } = self;
        bae_core::import::SheetGroup {
            sheet_id,
            name,
            path: std::path::PathBuf::from(local_path),
            bound: bound.into_core(),
            assignment: assignment.into_core(),
            disc_options,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetBound {
    fn from_core(bound: bae_core::import::SheetBound) -> Self {
        use bae_core::import::SheetBound;
        match bound {
            SheetBound::Describes(container) => Self::Describes {
                container: BridgeMappingContainer::from_core(container),
            },
            SheetBound::Unresolved { requested } => Self::Unresolved { requested },
            SheetBound::RefusedCodec { container, codec } => Self::RefusedCodec {
                container: BridgeMappingContainer::from_core(container),
                codec,
            },
        }
    }

    fn into_core(self) -> bae_core::import::SheetBound {
        use bae_core::import::SheetBound;
        match self {
            Self::Describes { container } => SheetBound::Describes(container.into_core()),
            Self::Unresolved { requested } => SheetBound::Unresolved { requested },
            Self::RefusedCodec { container, codec } => SheetBound::RefusedCodec {
                container: container.into_core(),
                codec,
            },
        }
    }
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
impl BridgeMappingRow {
    fn from_core(row: bae_core::import::MappingRow) -> Self {
        use bae_core::import::MappingRow;
        match row {
            MappingRow::Unit(unit) => Self::Unit {
                unit: BridgeMappingUnit::from_core(unit),
            },
            MappingRow::Sheet { sheet, entries } => Self::Sheet {
                sheet: BridgeSheetGroup::from_core(sheet),
                entries: entries
                    .into_iter()
                    .map(BridgeMappingUnit::from_core)
                    .collect(),
            },
            MappingRow::Directory(directory) => Self::Directory {
                directory: BridgeCollapsedDirectory::from_core(directory),
            },
        }
    }

    fn into_core(self) -> bae_core::import::MappingRow {
        use bae_core::import::MappingRow;
        match self {
            Self::Unit { unit } => MappingRow::Unit(unit.into_core()),
            Self::Sheet { sheet, entries } => MappingRow::Sheet {
                sheet: sheet.into_core(),
                entries: entries
                    .into_iter()
                    .map(BridgeMappingUnit::into_core)
                    .collect(),
            },
            Self::Directory { directory } => MappingRow::Directory(directory.into_core()),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingTable {
    pub(crate) fn from_core(table: bae_core::import::MappingTable) -> Self {
        let bae_core::import::MappingTable {
            images,
            rows,
            reconciliation,
        } = table;
        BridgeMappingTable {
            images: images
                .into_iter()
                .map(BridgeMappingImage::from_core)
                .collect(),
            rows: rows.into_iter().map(BridgeMappingRow::from_core).collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::from_core),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::MappingTable {
        let BridgeMappingTable {
            images,
            rows,
            reconciliation,
        } = self;
        bae_core::import::MappingTable {
            images: images
                .into_iter()
                .map(BridgeMappingImage::into_core)
                .collect(),
            rows: rows.into_iter().map(BridgeMappingRow::into_core).collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::into_core),
        }
    }
}
