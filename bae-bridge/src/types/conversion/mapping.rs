use super::super::*;

impl BridgeFileInfo {
    fn from_core(file: &bae_import::PaneFile) -> Self {
        BridgeFileInfo {
            name: file.name.clone(),
            size: file.size,
            file_name: file.file_name.clone(),
            audio_format: file.audio_format.clone().map(BridgeAudioFormat::from_core),
        }
    }
}

impl BridgeCandidateFile {
    fn from_core(entry: bae_import::PaneFile) -> Self {
        use bae_import::PaneFileRole;
        let file = BridgeFileInfo::from_core(&entry);
        let role = match entry.role {
            PaneFileRole::Audio => BridgeFileRole::Audio,
            PaneFileRole::TrackSheet { track_count } => BridgeFileRole::TrackSheet { track_count },
            PaneFileRole::Artwork => BridgeFileRole::Artwork,
            PaneFileRole::DamagedImage => BridgeFileRole::DamagedImage,
            PaneFileRole::Document => BridgeFileRole::Document,
            PaneFileRole::Other => BridgeFileRole::Other,
        };
        BridgeCandidateFile { file, role }
    }
}

mirror_enum! {
    BridgeSheetBindingOffer = bae_import::folder_scanner::SheetBindingOffer,
    from_core: pub(crate) fn,
    variants: { Offered, RefusedCodec { codec }, RefusedTiming, RefusedUnreadable },
}

mirror_struct! {
    BridgeSheetBindingOption = bae_import::folder_scanner::SheetBindingOption,
    from_core: pub(crate) fn,
    fields: { file_id, offer: (BridgeSheetBindingOffer) },
}

mirror_struct! {
    BridgeSheetReferenceOptions = bae_import::folder_scanner::SheetReferenceOptions,
    from_core: pub(crate) fn,
    fields: { file_reference, file_id, options: (each BridgeSheetBindingOption) },
}

impl BridgeCandidateFiles {
    /// Builds the pane's file list from core's files, cover files and audio
    /// summary, and keeps the identity of the audio files whose tags a
    /// file-metadata preview reads.
    pub(crate) fn from_core(
        file_metadata_identity: String,
        files: Vec<bae_import::PaneFile>,
        cover_files: Vec<bae_import::PaneFile>,
        source_audio: Option<bae_records::source_audio::SourceAudioSummary>,
    ) -> Self {
        BridgeCandidateFiles {
            file_metadata_identity,
            cover_files: cover_files.into_iter().map(BridgeCandidateFile::from_core).collect(),
            files: files.into_iter().map(BridgeCandidateFile::from_core).collect(),
            source_audio: source_audio.map(BridgeSourceAudioSummary::from_core),
        }
    }
}

impl BridgePressingEdit {
    pub(super) fn from_core(pressing: bae_types::pressing::Pressing) -> Self {
        let bae_types::pressing::Pressing { year, labels, barcodes, facts } = pressing;
        Self {
            year,
            labels: labels.into_iter().map(BridgeReleaseLabel::from_core).collect(),
            barcodes,
            facts: BridgePressingFacts::from_core(facts),
        }
    }
}

mirror_enum! {
    BridgeAudioFile = bae_import::AudioFile,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        Standalone { file_id },
        SheetSlice { file_id, sheet_id, index },
    },
}

mirror_struct! {
    BridgeMediumSide = bae_types::mediums::MediumSide,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { medium, side },
}

impl BridgeSheetPlace {
    pub(crate) fn into_core(self) -> bae_import::folder_scanner::SheetPlace {
        use bae_import::folder_scanner::SheetPlace;
        match self {
            Self::On { side } => SheetPlace::On(side.into_core()),
            Self::Ignored => SheetPlace::Ignored,
        }
    }
}

mirror_struct! {
    BridgeSheetSideChoice = bae_records::DbSheetSideChoice,
    from_core: pub(crate) fn,
    fields: { side: (BridgeMediumSide), heading: (BridgeTrackSide) },
}

mirror_enum! {
    BridgeMappingRole = bae_import::MappingRole,
    from_core: fn,
    variants: {
        Audio,
        Document,
        Image,
        DamagedImage,
        DamagedAudio { damage: (crate::types::BridgeAudioDamage) },
        Other,
    },
}

impl BridgeMappingFile {
    pub(crate) fn from_core(file: bae_import::MappingFile) -> Self {
        let bae_import::MappingFile { file_id, name, size, preview_target, role } = file;
        BridgeMappingFile {
            role: BridgeMappingRole::from_core(role),
            preview_target: preview_target.map(BridgePreviewTarget::from_core),
            file_id,
            name,
            size,
        }
    }
}

impl BridgeMappingEntry {
    fn from_core(entry: bae_import::MappingEntry) -> Self {
        let bae_import::MappingEntry {
            sheet_id,
            index,
            number,
            start_ms,
            container_id,
            preview_target,
        } = entry;
        BridgeMappingEntry {
            sheet_id,
            index,
            number,
            start: BridgeDurationClock::of_length(start_ms),
            container_id,
            preview_target: BridgePreviewTarget::from_core(preview_target),
        }
    }
}

mirror_enum! {
    BridgeMappingSource = bae_import::MappingSource,
    from_core: fn,
    variants: {
        File(file: (BridgeMappingFile)),
        SheetEntry(entry: (BridgeMappingEntry)),
    },
}

impl BridgeTrackMapping {
    fn from_core(mapping: bae_import::TrackMapping) -> Self {
        let bae_import::TrackMapping {
            source,
            track,
            position,
            length_ms,
            listed_length_ms,
            artists,
        } = mapping;
        BridgeTrackMapping {
            source: BridgeMappingSource::from_core(source),
            track: BridgeRawTrackEdit::from_core(track),
            position,
            length: BridgeDurationClock::of_length(length_ms),
            listed_length: listed_length_ms.map(BridgeDurationClock::of_length),
            artists: artists.map(BridgeArtistField::from_core),
        }
    }
}

mirror_struct! {
    BridgeMappingContainer = bae_import::MappingContainer,
    from_core: fn,
    fields: { file_id, name, size },
}

impl BridgeSheetGroup {
    fn from_core(sheet: bae_import::SheetGroup) -> Self {
        let bae_import::SheetGroup {
            sheet_id,
            name,
            size,
            bound,
            bound_audio,
            unbound_references,
            assigned,
            side_choices,
        } = sheet;
        BridgeSheetGroup {
            sheet_id,
            name,
            size,
            bound: BridgeSheetBound::from_core(bound),
            bound_audio: BridgeSheetBoundAudio::from_core(bound_audio),
            unbound_references: unbound_references.map(|references| {
                references.into_iter().map(BridgeSheetReferenceOptions::from_core).collect()
            }),
            assigned: assigned.map(BridgeSheetSideChoice::from_core),
            side_choices: side_choices.into_iter().map(BridgeSheetSideChoice::from_core).collect(),
        }
    }
}

mirror_enum! {
    BridgeSheetBoundAudio = bae_import::SheetBoundAudio,
    from_core: fn,
    variants: {
        None,
        One(reference: (BridgeSheetReferenceOptions)),
        Several(references: (each BridgeSheetReferenceOptions)),
    },
}

mirror_enum! {
    BridgeSheetBound = bae_import::SheetBound,
    from_core: fn,
    variants: {
        Describes(container: (BridgeMappingContainer)),
        DescribesFiles { audio_file_count },
        Unresolved { requested },
        RefusedCodec { codec },
        RefusedTiming,
    },
}

impl BridgeMappingImage {
    pub(crate) fn from_core(image: bae_import::MappingImage) -> Self {
        let bae_import::MappingImage { file_id, name, file } = image;
        BridgeMappingImage { file_id, name, file: BridgeFileVersion::from_core(file) }
    }
}

mirror_enum! {
    BridgeMappingTrackRun = bae_import::MappingTrackRun,
    from_core: fn,
    variants: {
        Tracks(mappings: (each BridgeTrackMapping)),
        Sheet {
            sheet: (BridgeSheetGroup),
            entries: (each BridgeTrackMapping),
        },
    },
}

mirror_struct! {
    BridgeMappingTrackSide = bae_import::MappingTrackSide,
    from_core: fn,
    fields: {
        side: (BridgeTrackSide),
        runs: (each BridgeMappingTrackRun),
    },
}

mirror_enum! {
    BridgeMappingFileRow = bae_import::MappingFileRow,
    from_core: fn,
    variants: {
        File(file: (BridgeMappingFile)),
        Sheet(sheet: (BridgeSheetGroup)),
    },
}

mirror_struct! {
    BridgeMappingTable = bae_import::MappingTable,
    from_core: pub(crate) fn,
    fields: {
        images: (each BridgeMappingImage),
        track_sides: (each BridgeMappingTrackSide),
        files: (each BridgeMappingFileRow),
        untitled_track_count,
    },
}
