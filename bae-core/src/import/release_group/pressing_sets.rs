//! The sets of records that name one physical pressing: gathered from the
//! evidence the records carry, or read off rows a run already formed.

use crate::import::pressing_evidence::{ComparedPressing, PressingEvidence, Support};
use crate::import::search::MetadataResult;

/// The sets of records each already-formed row holds, in the shape
/// [`gather_pressings`] names them: indexes into the result list, in list
/// order, and only the sets of two or more, since a record alone in its row
/// is a row of its own without one here.
pub(super) fn formed_pressings(rows: &[u32]) -> Vec<Vec<usize>> {
    let mut sets: Vec<(u32, Vec<usize>)> = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        match sets.iter_mut().find(|(named, _)| named == row) {
            Some((_, members)) => members.push(at),
            None => sets.push((*row, vec![at])),
        }
    }
    sets.into_iter()
        .map(|(_, members)| members)
        .filter(|members| members.len() > 1)
        .collect()
}

/// The sets of records that name one pressing, as indexes into `releases`,
/// each in the order its records arrived. Only the sets of two or more: a
/// record no other names is a row of its own without one here.
///
/// Every record is weighed against every other, and only records of two
/// different catalogs can name one pressing — see
/// [`PressingEvidence::support`]. Candidates are taken from the
/// best-supported level down. At
/// each level, the candidate edges between distinct sets that are still open
/// are read together: the sets an edge chain connects become one when every
/// record across them supports every other; where they do not, a set that
/// more than one of the chain's edges names is ambiguous and is settled as
/// it stands, and a set named once stays open for the levels below. Nothing
/// depends on the order the records arrived in.
pub(super) fn gather_pressings(releases: &[MetadataResult]) -> Vec<Vec<usize>> {
    let facts: Vec<ComparedPressing<'_>> = releases.iter().map(ComparedPressing::of).collect();
    let count = releases.len();
    let mut supported = vec![vec![false; count]; count];
    let mut edges: Vec<(Support, usize, usize)> = Vec::new();
    for a in 0..count {
        for b in a + 1..count {
            if let Some(support) = PressingEvidence::between(&facts[a], &facts[b]).support() {
                supported[a][b] = true;
                supported[b][a] = true;
                edges.push((support, a, b));
            }
        }
    }
    edges.sort_by(|(a, _, _), (b, _, _)| b.cmp(a));

    // A set is named by the lowest index in it, which holds its members.
    let mut set_of: Vec<usize> = (0..count).collect();
    let mut members: Vec<Vec<usize>> = (0..count).map(|at| vec![at]).collect();
    let mut settled = vec![false; count];
    let mut level = edges.as_slice();
    while let Some((top, _, _)) = level.first() {
        let end = level
            .iter()
            .position(|(other, _, _)| other != top)
            .unwrap_or(level.len());
        // The distinct pairs of open sets this level's edges connect.
        let mut live: Vec<(usize, usize)> = level[..end]
            .iter()
            .map(|(_, a, b)| (set_of[*a].min(set_of[*b]), set_of[*a].max(set_of[*b])))
            .filter(|(a, b)| a != b && !settled[*a] && !settled[*b])
            .collect();
        live.sort_unstable();
        live.dedup();
        for chain in chains(&live) {
            let all_support = chain.iter().enumerate().all(|(i, &x)| {
                chain[i + 1..].iter().all(|&y| {
                    members[x]
                        .iter()
                        .all(|&p| members[y].iter().all(|&q| supported[p][q]))
                })
            });
            if all_support {
                let into = chain[0];
                for &set in &chain[1..] {
                    let moved = std::mem::take(&mut members[set]);
                    for &member in &moved {
                        set_of[member] = into;
                    }
                    members[into].extend(moved);
                }
                members[into].sort_unstable();
            } else {
                for &set in &chain {
                    let named = live.iter().filter(|(a, b)| *a == set || *b == set).count();
                    if named > 1 {
                        settled[set] = true;
                    }
                }
            }
        }
        level = &level[end..];
    }
    members.into_iter().filter(|set| set.len() > 1).collect()
}

/// The groups of sets the edges connect, each sorted, in the order of their
/// lowest set.
fn chains(edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for &(a, b) in edges {
        let of_a = chains.iter().position(|chain| chain.contains(&a));
        let of_b = chains.iter().position(|chain| chain.contains(&b));
        match (of_a, of_b) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => {
                let (keep, drop) = (x.min(y), x.max(y));
                let moved = chains.remove(drop);
                chains[keep].extend(moved);
            }
            (Some(x), None) => chains[x].push(b),
            (None, Some(y)) => chains[y].push(a),
            (None, None) => chains.push(vec![a, b]),
        }
    }
    for chain in &mut chains {
        chain.sort_unstable();
    }
    chains.sort_by_key(|chain| chain[0]);
    chains
}
