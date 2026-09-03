use super::super::*;

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
impl BridgeCandidateSourceAudio {
    fn from_core(source_audio: bae_core::import::folder_scanner::CandidateSourceAudio<'_>) -> Self {
        let bae_core::import::folder_scanner::CandidateSourceAudio { summary, files } =
            source_audio;
        Self {
            summary: BridgeSourceAudioSummary::from_core(summary),
            files: files.into_iter().map(BridgeFileInfo::from_core).collect(),
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
                position,
                named_by_source,
            } => Self::Track {
                track: BridgeRawTrackEdit::from_core(track),
                position,
                named_by_source,
            },
            MappingBecomes::AwaitingPick => Self::AwaitingPick,
        }
    }

    fn into_core(self) -> bae_core::import::MappingBecomes {
        use bae_core::import::MappingBecomes;
        match self {
            Self::Track {
                track,
                position,
                named_by_source,
            } => MappingBecomes::Track {
                track: track.into_core(),
                position,
                named_by_source,
            },
            Self::AwaitingPick => MappingBecomes::AwaitingPick,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeTrackMapping {
    fn from_core(mapping: bae_core::import::TrackMapping) -> Self {
        let bae_core::import::TrackMapping {
            source,
            becomes,
            duration_ms,
        } = mapping;
        BridgeTrackMapping {
            source: BridgeMappingSource::from_core(source),
            becomes: BridgeMappingBecomes::from_core(becomes),
            duration_ms,
        }
    }

    fn into_core(self) -> bae_core::import::TrackMapping {
        let BridgeTrackMapping {
            source,
            becomes,
            duration_ms,
        } = self;
        bae_core::import::TrackMapping {
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
impl BridgeMappingTrackSectionContent {
    fn from_core(content: bae_core::import::MappingTrackSectionContent) -> Self {
        use bae_core::import::MappingTrackSectionContent;
        match content {
            MappingTrackSectionContent::Tracks(mappings) => Self::Tracks {
                mappings: mappings
                    .into_iter()
                    .map(BridgeTrackMapping::from_core)
                    .collect(),
            },
            MappingTrackSectionContent::Sheet { sheet, entries } => Self::Sheet {
                sheet: BridgeSheetGroup::from_core(sheet),
                entries: entries
                    .into_iter()
                    .map(BridgeTrackMapping::from_core)
                    .collect(),
            },
        }
    }

    fn into_core(self) -> bae_core::import::MappingTrackSectionContent {
        use bae_core::import::MappingTrackSectionContent;
        match self {
            Self::Tracks { mappings } => MappingTrackSectionContent::Tracks(
                mappings
                    .into_iter()
                    .map(BridgeTrackMapping::into_core)
                    .collect(),
            ),
            Self::Sheet { sheet, entries } => MappingTrackSectionContent::Sheet {
                sheet: sheet.into_core(),
                entries: entries
                    .into_iter()
                    .map(BridgeTrackMapping::into_core)
                    .collect(),
            },
        }
    }
}

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
impl BridgeMappingFileRow {
    fn from_core(row: bae_core::import::MappingFileRow) -> Self {
        use bae_core::import::MappingFileRow;
        match row {
            MappingFileRow::File(file) => Self::File {
                file: BridgeMappingFile::from_core(file),
            },
            MappingFileRow::Sheet(sheet) => Self::Sheet {
                sheet: BridgeSheetGroup::from_core(sheet),
            },
        }
    }

    fn into_core(self) -> bae_core::import::MappingFileRow {
        use bae_core::import::MappingFileRow;
        match self {
            Self::File { file } => MappingFileRow::File(file.into_core()),
            Self::Sheet { sheet } => MappingFileRow::Sheet(sheet.into_core()),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingTable {
    pub(crate) fn from_core(table: bae_core::import::MappingTable) -> Self {
        let bae_core::import::MappingTable {
            images,
            track_sections,
            files,
            reconciliation,
        } = table;
        BridgeMappingTable {
            images: images
                .into_iter()
                .map(BridgeMappingImage::from_core)
                .collect(),
            track_sections: track_sections
                .into_iter()
                .map(BridgeMappingTrackSection::from_core)
                .collect(),
            files: files
                .into_iter()
                .map(BridgeMappingFileRow::from_core)
                .collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::from_core),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::MappingTable {
        let BridgeMappingTable {
            images,
            track_sections,
            files,
            reconciliation,
        } = self;
        bae_core::import::MappingTable {
            images: images
                .into_iter()
                .map(BridgeMappingImage::into_core)
                .collect(),
            track_sections: track_sections
                .into_iter()
                .map(BridgeMappingTrackSection::into_core)
                .collect(),
            files: files
                .into_iter()
                .map(BridgeMappingFileRow::into_core)
                .collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::into_core),
        }
    }
}
