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
    ImportCandidateMetadataProvenanceSelect,
    ImportCandidateEditFieldSet,
    ImportCandidateCoverSet,
    ImportStart,
    ReleaseDetailGet,
    ReleaseExport,
    ReleaseStorageAction,
    OutputStatus,
    ReleaseReidentify,
    ReleaseMetadataReset,
    ReleaseMetadataUpdate,
    LibrarySearch,
}

impl AutomationTool {
    const DESCRIPTORS: [AutomationToolDescriptor; 21] = [
        AutomationToolDescriptor {
            tool: AutomationTool::ConfigGet,
            name: "config_get",
            description: "Get active library automation config",
            schema: None,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFoldersList,
            name: "watched_folders_list",
            description: "List watched import folders",
            schema: None,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFolderAdd,
            name: "watched_folder_add",
            description: "Add a watched import folder",
            schema: Some(schema_object::<PathInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFolderRemove,
            name: "watched_folder_remove",
            description: "Remove a watched import folder",
            schema: Some(schema_object::<PathInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFoldersScan,
            name: "watched_folders_scan",
            description: "Scan watched import folders",
            schema: Some(schema_object::<ScanWait>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidatesList,
            name: "import_candidates_list",
            description: "List indexed import candidates",
            schema: None,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateGet,
            name: "import_candidate_get",
            description: "Get an indexed import candidate",
            schema: Some(schema_object::<CandidateKeyInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateSkipSet,
            name: "import_candidate_skip_set",
            description: "Set candidate skipped state",
            schema: Some(schema_object::<CandidateSkipSetInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportSearch,
            name: "import_search",
            description: "Search metadata sources for import",
            schema: Some(schema_object::<AutomationSearchQuery>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateMetadataProvenanceSelect,
            name: "import_candidate_metadata_provenance_select",
            description: "Select external release, file tags, or manual entry as a candidate's metadata source",
            schema: Some(schema_object::<CandidateMetadataProvenanceInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateEditFieldSet,
            name: "import_candidate_edit_field_set",
            description: "Type one album-level metadata field over what the candidate's pick seeds",
            schema: Some(schema_object::<CandidateEditFieldInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateCoverSet,
            name: "import_candidate_cover_set",
            description: "Choose the cover a candidate commits with",
            schema: Some(schema_object::<CandidateCoverInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportStart,
            name: "import_start",
            description: "Start an import of a candidate from what it stores: its pick, its metadata edits, its track rows and its cover",
            schema: Some(schema_object::<AutomationStartImport>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseDetailGet,
            name: "release_detail_get",
            description: "Get library release detail",
            schema: Some(schema_object::<ReleaseIdInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseExport,
            name: "release_export",
            description: "Enqueue a byte-accurate export of a release's files to a directory",
            schema: Some(schema_object::<ReleaseExportInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseStorageAction,
            name: "release_storage_action",
            description: "Run a release storage transition: move to cloud (optionally pinned), pin, unpin, make local, or cancel the one in flight",
            schema: Some(schema_object::<ReleaseStorageActionInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::OutputStatus,
            name: "output_status",
            description: "Get the export queue snapshot (per-release progress)",
            schema: None,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseReidentify,
            name: "release_reidentify",
            description: "Set release identity",
            schema: Some(schema_object::<ReleaseReidentifyInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseMetadataReset,
            name: "release_metadata_reset",
            description: "Project release metadata from its source",
            schema: Some(schema_object::<ReleaseIdInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseMetadataUpdate,
            name: "release_metadata_update",
            description: "Apply release metadata edit",
            schema: Some(schema_object::<ReleaseMetadataUpdateInput>),
        },
        AutomationToolDescriptor {
            tool: AutomationTool::LibrarySearch,
            name: "library_search",
            description: "Search the library",
            schema: Some(schema_object::<LibrarySearchInput>),
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
        match self.descriptor().schema {
            Some(schema) => schema(),
            None => empty_input_schema(),
        }
    }

    /// A tool with no schema takes no arguments, so an MCP call that omits them
    /// entirely is well-formed rather than a missing-argument error.
    pub fn accepts_missing_arguments(&self) -> bool {
        self.descriptor().schema.is_none()
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
    /// The tool's input shape, as the function that emits its JSON schema.
    /// `None` is a tool that takes no arguments.
    schema: Option<fn() -> Map<String, Value>>,
}
