use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationTool {
    ConfigGet,
    WatchedFoldersList,
    WatchedFolderAdd,
    WatchedFolderRemove,
    WatchedFoldersScan,
    ImportCandidatesList,
    ImportCandidateGet,
    ImportCandidateSkipSet,
    ImportSearch,
    ImportReleasePrefetch,
    ImportFileTagsPreview,
    ImportReleaseEditShape,
    ImportStart,
    ReleaseDetailGet,
    ReleaseExport,
    OutputStatus,
    ReleaseReidentify,
    ReleaseMetadataReset,
    ReleaseMetadataUpdate,
    LibrarySearch,
}

impl AutomationTool {
    const DESCRIPTORS: [AutomationToolDescriptor; 20] = [
        AutomationToolDescriptor {
            tool: AutomationTool::ConfigGet,
            name: "config_get",
            description: "Get active library automation config",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFoldersList,
            name: "watched_folders_list",
            description: "List watched import folders",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFolderAdd,
            name: "watched_folder_add",
            description: "Add a watched import folder",
            input: AutomationToolInput::Path,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFolderRemove,
            name: "watched_folder_remove",
            description: "Remove a watched import folder",
            input: AutomationToolInput::Path,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFoldersScan,
            name: "watched_folders_scan",
            description: "Scan watched import folders",
            input: AutomationToolInput::ScanWait,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidatesList,
            name: "import_candidates_list",
            description: "List indexed import candidates",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateGet,
            name: "import_candidate_get",
            description: "Get an indexed import candidate",
            input: AutomationToolInput::CandidateKey,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateSkipSet,
            name: "import_candidate_skip_set",
            description: "Set candidate skipped state",
            input: AutomationToolInput::CandidateSkipSet,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportSearch,
            name: "import_search",
            description: "Search metadata sources for import",
            input: AutomationToolInput::SearchQuery,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportReleasePrefetch,
            name: "import_release_prefetch",
            description: "Prefetch a release for a candidate: display detail, the identity claim the pick implies, and the editor seed before that claim masks it",
            input: AutomationToolInput::ReleasePrefetch,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportFileTagsPreview,
            name: "import_file_tags_preview",
            description: "Preview file-tag metadata for a folder",
            input: AutomationToolInput::Folder,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportReleaseEditShape,
            name: "import_release_edit_shape",
            description: "Shape a prefetched editor seed for an identity choice",
            input: AutomationToolInput::ShapeReleaseEdit,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportStart,
            name: "import_start",
            description: "Start an import through the core import service",
            input: AutomationToolInput::StartImport,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseDetailGet,
            name: "release_detail_get",
            description: "Get library release detail",
            input: AutomationToolInput::ReleaseId,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseExport,
            name: "release_export",
            description: "Enqueue a byte-accurate export of a release's files to a directory",
            input: AutomationToolInput::ReleaseExport,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::OutputStatus,
            name: "output_status",
            description: "Get the export queue snapshot (per-release progress)",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseReidentify,
            name: "release_reidentify",
            description: "Set release identity",
            input: AutomationToolInput::ReleaseReidentify,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseMetadataReset,
            name: "release_metadata_reset",
            description: "Project release metadata from its source",
            input: AutomationToolInput::ReleaseId,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseMetadataUpdate,
            name: "release_metadata_update",
            description: "Apply release metadata edit",
            input: AutomationToolInput::ReleaseMetadataUpdate,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::LibrarySearch,
            name: "library_search",
            description: "Search the library",
            input: AutomationToolInput::LibrarySearch,
        },
    ];

    pub fn all() -> impl Iterator<Item = Self> {
        Self::DESCRIPTORS.iter().map(|descriptor| descriptor.tool)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.name == name)
            .map(|descriptor| descriptor.tool)
    }

    pub fn name(&self) -> &'static str {
        self.descriptor().name
    }

    pub fn description(&self) -> &'static str {
        self.descriptor().description
    }

    pub fn input_schema(&self) -> Map<String, Value> {
        self.descriptor().input.schema()
    }

    pub fn accepts_missing_arguments(&self) -> bool {
        self.descriptor().input.accepts_missing_arguments()
    }

    fn descriptor(&self) -> &'static AutomationToolDescriptor {
        Self::DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.tool == *self)
            .expect("automation tool descriptor")
    }
}

#[derive(Debug, Clone, Copy)]
struct AutomationToolDescriptor {
    tool: AutomationTool,
    name: &'static str,
    description: &'static str,
    input: AutomationToolInput,
}

#[derive(Debug, Clone, Copy)]
enum AutomationToolInput {
    Empty,
    Path,
    ScanWait,
    CandidateKey,
    CandidateSkipSet,
    SearchQuery,
    ReleasePrefetch,
    Folder,
    ShapeReleaseEdit,
    StartImport,
    ReleaseId,
    ReleaseExport,
    ReleaseReidentify,
    ReleaseMetadataUpdate,
    LibrarySearch,
}

impl AutomationToolInput {
    fn schema(&self) -> Map<String, Value> {
        match self {
            Self::Empty => empty_input_schema(),
            Self::Path => schema_object::<PathInput>(),
            Self::ScanWait => schema_object::<ScanWait>(),
            Self::CandidateKey => schema_object::<CandidateKeyInput>(),
            Self::CandidateSkipSet => schema_object::<CandidateSkipSetInput>(),
            Self::SearchQuery => schema_object::<AutomationSearchQuery>(),
            Self::ReleasePrefetch => schema_object::<ReleasePrefetchInput>(),
            Self::Folder => schema_object::<FolderInput>(),
            Self::ShapeReleaseEdit => schema_object::<ShapeReleaseEditInput>(),
            Self::StartImport => schema_object::<AutomationStartImport>(),
            Self::ReleaseId => schema_object::<ReleaseIdInput>(),
            Self::ReleaseExport => schema_object::<ReleaseExportInput>(),
            Self::ReleaseReidentify => schema_object::<ReleaseReidentifyInput>(),
            Self::ReleaseMetadataUpdate => schema_object::<ReleaseMetadataUpdateInput>(),
            Self::LibrarySearch => schema_object::<LibrarySearchInput>(),
        }
    }

    fn accepts_missing_arguments(&self) -> bool {
        matches!(self, Self::Empty)
    }
}
