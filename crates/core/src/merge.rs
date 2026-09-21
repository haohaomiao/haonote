//! Conservative three-way merge; never resolves ambiguity by wall-clock order.
use crate::{Content, Revision};
use std::collections::{HashMap, HashSet};

fn field<T: Eq + Clone>(base: &T, left: &T, right: &T) -> Option<T> {
    if left == right || right == base {
        Some(left.clone())
    } else if left == base {
        Some(right.clone())
    } else {
        None
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Edit<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
}

fn edits<'a>(base: &[&str], changed: &[&'a str]) -> Option<Vec<Edit<'a>>> {
    let width = changed.len() + 1;
    let cells = (base.len() + 1).checked_mul(width)?;
    if cells > 1_000_000 {
        return None;
    }
    let mut lcs = vec![0u32; cells];
    for i in (0..base.len()).rev() {
        for j in (0..changed.len()).rev() {
            lcs[i * width + j] = if base[i] == changed[j] {
                1 + lcs[(i + 1) * width + j + 1]
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut result = Vec::new();
    while i < base.len() || j < changed.len() {
        if i < base.len() && j < changed.len() && base[i] == changed[j] {
            i += 1;
            j += 1;
            continue;
        }
        let mut edit = Edit {
            start: i,
            end: i,
            lines: Vec::new(),
        };
        while i < base.len() || j < changed.len() {
            if i < base.len() && j < changed.len() && base[i] == changed[j] {
                break;
            }
            if j < changed.len()
                && (i == base.len() || lcs[i * width + j + 1] > lcs[(i + 1) * width + j])
            {
                edit.lines.push(changed[j]);
                j += 1;
            } else {
                i += 1;
            }
        }
        edit.end = i;
        result.push(edit);
    }
    Some(result)
}

fn structured(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
            || line.contains('|')
            || line.starts_with("    ")
            || line.starts_with('\t')
            || trimmed.starts_with('<')
    })
}

fn text(base: &str, left: &str, right: &str) -> Option<String> {
    if let Some(result) = field(&base, &left, &right) {
        return Some(result.to_owned());
    }
    if [base, left, right].iter().any(|s| structured(s)) {
        return None;
    }
    let lines: Vec<_> = base.split_inclusive('\n').collect();
    let mut all = edits(&lines, &left.split_inclusive('\n').collect::<Vec<_>>())?;
    for b in edits(&lines, &right.split_inclusive('\n').collect::<Vec<_>>())? {
        if all.contains(&b) {
            continue;
        }
        for a in &all {
            let overlap = if a.start == a.end || b.start == b.end {
                a.start <= b.end && b.start <= a.end
            } else {
                a.start < b.end && b.start < a.end
            };
            if overlap {
                return None;
            }
        }
        all.push(b);
    }
    all.sort_by_key(|e| (e.start, e.end));
    let mut result = String::new();
    let mut cursor = 0;
    for edit in all {
        result.push_str(&lines[cursor..edit.start].concat());
        result.push_str(&edit.lines.concat());
        cursor = edit.end;
    }
    result.push_str(&lines[cursor..].concat());
    Some(result)
}

fn content(base: &Content, left: &Content, right: &Content) -> Option<Content> {
    // Delete/archive versus concurrent edits must be explicitly reviewed.
    if left.deleted != right.deleted || left.archived != right.archived {
        return None;
    }
    let result = Content {
        text: text(&base.text, &left.text, &right.text)?,
        title: field(&base.title, &left.title, &right.title)?,
        color: field(&base.color, &left.color, &right.color)?,
        deleted: left.deleted,
        archived: left.archived,
    };
    result.validate().ok()?;
    Some(result)
}

fn ancestors<'a>(id: &'a str, map: &HashMap<&'a str, &'a Revision>) -> Option<HashSet<&'a str>> {
    let mut done = HashSet::new();
    let mut active = HashSet::new();
    let mut stack = vec![(id, false)];
    while let Some((id, leaving)) = stack.pop() {
        if leaving {
            active.remove(id);
            done.insert(id);
            continue;
        }
        if done.contains(id) {
            continue;
        }
        if !active.insert(id) {
            return None;
        }
        let r = map.get(id)?; // Missing ancestors: defer until a later complete sync.
        stack.push((id, true));
        stack.extend(r.parents.iter().map(|p| (p.as_str(), false)));
    }
    Some(done)
}

pub(crate) fn merge(revisions: &[Revision], heads: &[&Revision]) -> Option<Revision> {
    if !(2..=64).contains(&heads.len()) {
        return None;
    }
    let map: HashMap<_, _> = revisions.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut ordered = heads.to_vec();
    ordered.sort_by_key(|r| &r.id);
    let ancestry: Vec<_> = ordered
        .iter()
        .map(|r| ancestors(&r.id, &map))
        .collect::<Option<_>>()?;
    let common: Vec<_> = ancestry[0]
        .iter()
        .copied()
        .filter(|id| ancestry.iter().all(|set| set.contains(id)))
        .collect();
    // A unique common ancestor must descend from every other common ancestor.
    let older: HashSet<_> = common
        .iter()
        .flat_map(|id| map[id].parents.iter().map(String::as_str))
        .collect();
    let bases: Vec<_> = common
        .into_iter()
        .filter(|id| !older.contains(id))
        .collect();
    if bases.len() != 1 {
        return None;
    }
    let base = map[bases[0]];
    let mut merged = ordered[0].content.clone();
    for head in &ordered[1..] {
        merged = content(&base.content, &merged, &head.content)?;
    }
    let parents: Vec<_> = ordered.iter().map(|r| r.id.clone()).collect();
    // Devices merging the same heads produce exactly the same immutable revision.
    let seed =
        serde_json::to_string(&("haonote-auto-merge-v1", &base.note_id, &parents, &merged)).ok()?;
    let latest = ordered.iter().max_by_key(|r| {
        (
            chrono::DateTime::parse_from_rfc3339(&r.updated_at).ok(),
            &r.id,
        )
    })?;
    Some(Revision {
        schema: 1,
        id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, seed.as_bytes()).to_string(),
        note_id: base.note_id.clone(),
        parents,
        content: merged,
        created_at: base.created_at.clone(),
        updated_at: latest.updated_at.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_branches_fields_and_incomplete_ancestry() {
        let base = Revision {
            schema: 1,
            id: "base".into(),
            note_id: "note".into(),
            parents: vec![],
            content: Content::default(),
            created_at: "2026-09-17T00:00:00Z".into(),
            updated_at: "2026-09-17T00:00:00Z".into(),
        };
        let branch = |id: &str| Revision {
            id: id.into(),
            parents: vec![base.id.clone()],
            ..base.clone()
        };
        let mut a = branch("a");
        a.content.title = "title".into();
        let mut b = branch("b");
        b.content.text = "body".into();
        let mut c = branch("c");
        c.content.color = "sage".into();
        let all = vec![base.clone(), a.clone(), b.clone(), c.clone()];
        let merged = merge(&all, &[&a, &b, &c]).unwrap();
        assert_eq!(merged.content.title, "title");
        assert_eq!(merged.content.text, "body");
        assert_eq!(merged.content.color, "sage");
        assert_eq!(merge(&all, &[&c, &b, &a]).unwrap(), merged);
        assert!(merge(&all[1..], &[&a, &b]).is_none());
        let mut cyclic = all.clone();
        cyclic[0].parents = vec![a.id.clone()];
        assert!(merge(&cyclic, &[&a, &b]).is_none());
        let mut fork_a = branch("fork-a");
        fork_a.parents = vec![a.id.clone(), b.id.clone()];
        let mut fork_b = fork_a.clone();
        fork_b.id = "fork-b".into();
        let mut criss_cross = all;
        criss_cross.extend([fork_a.clone(), fork_b.clone()]);
        assert!(merge(&criss_cross, &[&fork_a, &fork_b]).is_none());
    }
    #[test]
    fn independent_lines_and_same_edits() {
        assert_eq!(
            text("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n").as_deref(),
            Some("A\nb\nC\n")
        );
        assert_eq!(text("a\nb", "a\nB", "a\nB").as_deref(), Some("a\nB"));
        assert!(text("a\nb", "a\nB", "a\nC").is_none());
        assert!(text("a\nb", "a\nx\nb", "a\ny\nb").is_none());
        assert_eq!(text("a\nb\nc", "b\nc", "a\nb\nC").as_deref(), Some("b\nC"));
    }
    #[test]
    fn structure_and_delete_are_conservative() {
        assert!(text("```\na\nb\n```", "```\nA\nb\n```", "```\na\nB\n```").is_none());
        let base = Content::default();
        let mut a = base.clone();
        let mut b = base.clone();
        a.deleted = true;
        b.text = "edit".into();
        assert!(content(&base, &a, &b).is_none());
    }
}
