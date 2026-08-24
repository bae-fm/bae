//! The stamp a settled reading leaves on the nodes below it.
//!
//! Split out of the walk because it does not walk: it works from the nodes the
//! walk has already produced.

use super::scan::ProjectedScanNode;
use super::*;

pub(super) fn apply_resolved_boundary(
    nodes: &mut [ProjectedScanNode],
    resolved: &ResolvedFolderReleaseBoundary,
) {
    for node in nodes {
        let resolved_boundaries = match node {
            ProjectedScanNode::Candidate(candidate) => &mut candidate.resolved_boundaries,
            ProjectedScanNode::Invalid(candidate) => &mut candidate.resolved_boundaries,
        };
        if !resolved_boundaries
            .iter()
            .any(|existing| existing.key == resolved.key)
        {
            resolved_boundaries.push(resolved.clone());
        }
    }
}
