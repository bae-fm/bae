//! What a folder that holds several releases says about itself: the keys of
//! the candidates under it, the tree a reader shows to choose between them,
//! and the stamp a settled reading leaves on each of them.
//!
//! Split out of the walk because none of it walks: every function here reads
//! the nodes the walk has already produced.

use super::scan::{directory_name, relative_path_string, ProjectedScanNode};
use super::*;
use std::collections::HashMap;
use std::path::Path;

pub(super) fn candidate_keys(nodes: &[ProjectedScanNode]) -> Vec<String> {
    let mut keys = Vec::new();
    for node in nodes {
        match node {
            ProjectedScanNode::Candidate(candidate) => {
                keys.push(candidate.path.to_string_lossy().into_owned())
            }
            ProjectedScanNode::Invalid(candidate) => {
                keys.push(candidate.path.to_string_lossy().into_owned())
            }
            ProjectedScanNode::Boundary(boundary) => {
                keys.extend(boundary.candidate_keys.iter().cloned())
            }
        }
    }
    keys
}

pub(super) fn boundary_tree_rows(
    root: &Path,
    boundary: &Path,
    watched_folder_path: &str,
    nodes: &[ProjectedScanNode],
) -> Vec<FolderReleaseTreeRow> {
    let boundary_relative = boundary
        .strip_prefix(root)
        .expect("a release boundary is below its watched root");
    let mut candidate_summaries = HashMap::new();
    let mut invalid_reasons = HashMap::new();
    for node in nodes {
        match node {
            ProjectedScanNode::Candidate(candidate) => {
                candidate_summaries.insert(
                    candidate.path.clone(),
                    FolderReleaseCandidateSummary {
                        track_count: candidate.track_count(),
                        format_label: candidate.files.format_label.clone(),
                    },
                );
            }
            ProjectedScanNode::Invalid(candidate) => {
                invalid_reasons.insert(candidate.path.clone(), candidate.reason.clone());
            }
            ProjectedScanNode::Boundary(nested) => {
                let nested_root = PathBuf::from(&nested.key.watched_folder_path)
                    .join(&nested.key.relative_folder_path);
                for row in &nested.tree_rows {
                    let absolute = nested_root.join(&row.display_path);
                    match &row.kind {
                        FolderReleaseTreeRowKind::Candidate { summary } => {
                            candidate_summaries.insert(absolute, summary.clone());
                        }
                        FolderReleaseTreeRowKind::Invalid { reason } => {
                            invalid_reasons.insert(absolute, reason.clone());
                        }
                        FolderReleaseTreeRowKind::Folder => {}
                    }
                }
            }
        }
    }
    let mut releases = BTreeSet::new();
    for key in candidate_keys(nodes) {
        releases.insert(PathBuf::from(key));
    }
    let mut descendant_counts: BTreeMap<PathBuf, u32> = BTreeMap::new();
    for absolute in &releases {
        let relative = absolute
            .strip_prefix(boundary)
            .expect("a boundary candidate is below its release boundary");
        let components: Vec<_> = relative.components().collect();
        for end in 0..components.len() {
            let path: PathBuf = components[..=end]
                .iter()
                .map(|component| component.as_os_str())
                .collect();
            *descendant_counts.entry(path).or_default() += 1;
        }
    }
    let mut rows: BTreeMap<String, FolderReleaseTreeRow> = BTreeMap::new();
    let boundary_kind = candidate_summaries
        .get(boundary)
        .cloned()
        .map(|summary| FolderReleaseTreeRowKind::Candidate { summary })
        .or_else(|| {
            invalid_reasons
                .get(boundary)
                .cloned()
                .map(|reason| FolderReleaseTreeRowKind::Invalid { reason })
        });
    if let Some(kind) = boundary_kind {
        let decision_key = FolderReleaseDecisionKey {
            watched_folder_path: watched_folder_path.to_string(),
            relative_folder_path: relative_path_string(boundary_relative),
        };
        rows.insert(
            String::new(),
            FolderReleaseTreeRow {
                name: directory_name(root, boundary),
                display_path: String::new(),
                depth: 0,
                kind,
                decision_key,
                ancestor_decision_keys: Vec::new(),
            },
        );
    }
    let descendant_depth_offset = u32::from(rows.contains_key(""));
    for absolute in releases {
        let relative = absolute
            .strip_prefix(boundary)
            .expect("a boundary candidate is below its release boundary");
        let components: Vec<_> = relative.components().collect();
        for end in 0..components.len() {
            let path: PathBuf = components[..=end]
                .iter()
                .map(|component| component.as_os_str())
                .collect();
            let display_path = relative_path_string(&path);
            let decision_key = FolderReleaseDecisionKey {
                watched_folder_path: watched_folder_path.to_string(),
                relative_folder_path: relative_path_string(&boundary_relative.join(&path)),
            };
            let mut ancestor_decision_keys = vec![FolderReleaseDecisionKey {
                watched_folder_path: watched_folder_path.to_string(),
                relative_folder_path: relative_path_string(boundary_relative),
            }];
            for prefix_end in 0..end {
                let prefix: PathBuf = components[..=prefix_end]
                    .iter()
                    .map(|component| component.as_os_str())
                    .collect();
                if descendant_counts
                    .get(&prefix)
                    .is_some_and(|count| *count > 1)
                {
                    ancestor_decision_keys.push(FolderReleaseDecisionKey {
                        watched_folder_path: watched_folder_path.to_string(),
                        relative_folder_path: relative_path_string(&boundary_relative.join(prefix)),
                    });
                }
            }
            let is_release = end + 1 == components.len();
            let kind = if is_release {
                if let Some(summary) = candidate_summaries.get(&absolute) {
                    FolderReleaseTreeRowKind::Candidate {
                        summary: summary.clone(),
                    }
                } else if let Some(reason) = invalid_reasons.get(&absolute) {
                    FolderReleaseTreeRowKind::Invalid {
                        reason: reason.clone(),
                    }
                } else {
                    FolderReleaseTreeRowKind::Folder
                }
            } else {
                FolderReleaseTreeRowKind::Folder
            };
            let row = FolderReleaseTreeRow {
                name: components[end].as_os_str().to_string_lossy().into_owned(),
                display_path: display_path.clone(),
                depth: end as u32 + descendant_depth_offset,
                kind,
                decision_key,
                ancestor_decision_keys,
            };
            rows.entry(display_path)
                .and_modify(|existing| {
                    if is_release {
                        existing.kind = row.kind.clone();
                    }
                })
                .or_insert(row);
        }
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|left, right| {
        natord::compare_ignore_case(&left.display_path, &right.display_path)
    });
    rows
}

pub(super) fn apply_resolved_boundary(
    nodes: &mut [ProjectedScanNode],
    resolved: &ResolvedFolderReleaseBoundary,
) {
    for node in nodes {
        let resolved_boundaries = match node {
            ProjectedScanNode::Candidate(candidate) => &mut candidate.resolved_boundaries,
            ProjectedScanNode::Invalid(candidate) => &mut candidate.resolved_boundaries,
            ProjectedScanNode::Boundary(_) => continue,
        };
        if !resolved_boundaries
            .iter()
            .any(|existing| existing.key == resolved.key)
        {
            resolved_boundaries.push(resolved.clone());
        }
    }
}
