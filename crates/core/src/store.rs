use crate::model::*;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

pub struct Store {
    conn: Connection,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: String,
    version: u8,
    revisions: Vec<Revision>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 1 {
            bail!("资料库由更新版本的haonote创建，请升级应用后再打开");
        }
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS revisions(id TEXT PRIMARY KEY, note_id TEXT NOT NULL, body TEXT NOT NULL, uploaded INTEGER NOT NULL DEFAULT 0);
            CREATE INDEX IF NOT EXISTS revisions_note ON revisions(note_id);
            CREATE TABLE IF NOT EXISTS drafts(note_id TEXT PRIMARY KEY, body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            PRAGMA user_version=1;")?;
        // Rebuildable metadata only: keep original revisions and the sync format intact.
        // Triggers also keep the index correct if an older app writes this database.
        conn.execute_batch("BEGIN IMMEDIATE;")?;
        let indexed: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='revision_parents')", [], |row| row.get(0))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS revision_parents(note_id TEXT NOT NULL, parent_id TEXT NOT NULL, PRIMARY KEY(note_id,parent_id));
            CREATE INDEX IF NOT EXISTS revisions_pending ON revisions(note_id) WHERE uploaded=0;
            CREATE TRIGGER IF NOT EXISTS index_revision_parents AFTER INSERT ON revisions BEGIN
                INSERT OR IGNORE INTO revision_parents(note_id,parent_id)
                SELECT NEW.note_id, value FROM json_each(NEW.body,'$.parents');
            END;")?;
        if !indexed {
            conn.execute_batch(
                "INSERT OR IGNORE INTO revision_parents(note_id,parent_id)
                SELECT r.note_id, p.value FROM revisions r, json_each(r.body,'$.parents') p;",
            )?;
        }
        conn.execute_batch("COMMIT;")?;
        Ok(Self { conn })
    }

    pub fn all(&self) -> Result<Vec<Revision>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body FROM revisions UNION ALL SELECT body FROM drafts")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    pub fn list(&self) -> Result<Vec<Note>> {
        self.list_for(None)
    }

    /// Only deserialize live branches, never the historical document bodies.
    fn current_revisions(&self, note_id: Option<&str>) -> Result<Vec<Revision>> {
        let filter = if note_id.is_some() {
            " WHERE note_id=?1"
        } else {
            ""
        };
        let sql = format!("WITH candidates AS (
            SELECT id,note_id,body FROM revisions{filter}
            UNION ALL SELECT json_extract(body,'$.id'),note_id,body FROM drafts{filter}
        ) SELECT c.body FROM candidates c
        WHERE NOT EXISTS(SELECT 1 FROM revision_parents p WHERE p.note_id=c.note_id AND p.parent_id=c.id)
        AND NOT EXISTS(SELECT 1 FROM drafts d, json_each(d.body,'$.parents') p WHERE d.note_id=c.note_id AND p.value=c.id)");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(note_id), |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    fn list_for(&self, note_id: Option<&str>) -> Result<Vec<Note>> {
        let mut grouped: BTreeMap<String, Vec<Revision>> = BTreeMap::new();
        for r in self.current_revisions(note_id)? {
            grouped.entry(r.note_id.clone()).or_default().push(r);
        }
        let mut notes = Vec::new();
        for (id, rs) in grouped {
            let mut heads = heads(&rs);
            // Stable across devices; conflicts remain visible even when the winning head is deleted.
            heads.sort_by(|a, b| (&a.updated_at, &a.id).cmp(&(&b.updated_at, &b.id)));
            if let Some(head) = heads.pop() {
                notes.push(Note {
                    id: id.clone(),
                    head_id: head.id.clone(),
                    content: head.content.clone(),
                    created_at: head.created_at.clone(),
                    updated_at: head.updated_at.clone(),
                    conflicts: heads.into_iter().cloned().collect(),
                    pending: self.conn.query_row("SELECT EXISTS(SELECT 1 FROM revisions WHERE note_id=?1 AND uploaded=0) OR EXISTS(SELECT 1 FROM drafts WHERE note_id=?1)", [&id], |row| row.get(0))?,
                });
            }
        }
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        Ok(notes)
    }

    pub fn note(&self, note_id: &str) -> Result<Note> {
        self.list_for(Some(note_id))?
            .into_iter()
            .find(|n| n.id == note_id)
            .context("便签不存在")
    }

    pub fn save(
        &mut self,
        note_id: Option<&str>,
        expected: Option<&str>,
        content: Content,
    ) -> Result<Note> {
        content.validate()?;
        let note_id = note_id.map(str::to_owned).unwrap_or_else(id);
        uuid::Uuid::parse_str(&note_id)?;
        let rs = self.current_revisions(Some(&note_id))?;
        let hs = heads(&rs);
        let base = match expected {
            Some(e) => Some(
                *hs.iter()
                    .find(|r| r.id == e)
                    .context("便签已在其他窗口更新。你的输入仍保留，请复制后重新打开便签")?,
            ),
            None if rs.is_empty() => None,
            _ => bail!("保存需要当前便签版本"),
        };
        if base.is_some_and(|r| r.content == content) {
            return self.note_at(base.unwrap());
        }
        let draft: Option<String> = self
            .conn
            .query_row("SELECT body FROM drafts WHERE note_id=?", [&note_id], |r| {
                r.get(0)
            })
            .optional()?;
        let draft: Option<Revision> = draft.map(|s| serde_json::from_str(&s)).transpose()?;
        let parent_ids = match (&draft, base) {
            (Some(d), Some(b)) if d.id == b.id => d.parents.clone(),
            (_, Some(b)) => vec![b.id.clone()],
            _ => vec![],
        };
        let r = Revision {
            schema: 1,
            id: id(),
            note_id: note_id.clone(),
            parents: parent_ids,
            content,
            created_at: base.map(|b| b.created_at.clone()).unwrap_or_else(now),
            updated_at: now(),
        };
        let tx = self.conn.transaction()?;
        // Coalesce only unsealed local edits; a revision sent to the network is never rewritten.
        if let Some(d) = draft {
            if base.is_none_or(|b| b.id != d.id) {
                tx.execute(
                    "INSERT INTO revisions(id,note_id,body) VALUES(?,?,?)",
                    params![d.id, d.note_id, serde_json::to_string(&d)?],
                )?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO drafts(note_id,body) VALUES(?,?)",
            params![note_id, serde_json::to_string(&r)?],
        )?;
        tx.commit()?;
        self.note_at(&r)
    }

    /// A save continues the edited branch even when another device's clock sorts later.
    fn note_at(&self, revision: &Revision) -> Result<Note> {
        let revisions = self.current_revisions(Some(&revision.note_id))?;
        Ok(Note {
            id: revision.note_id.clone(),
            head_id: revision.id.clone(),
            content: revision.content.clone(),
            created_at: revision.created_at.clone(),
            updated_at: revision.updated_at.clone(),
            conflicts: heads(&revisions)
                .into_iter()
                .filter(|r| r.id != revision.id)
                .cloned()
                .collect(),
            pending: self.note(&revision.note_id)?.pending,
        })
    }

    pub fn resolve(
        &mut self,
        note_id: &str,
        expected_heads: Vec<String>,
        content: Content,
    ) -> Result<Note> {
        content.validate()?;
        let rs = self.current_revisions(Some(note_id))?;
        let hs = heads(&rs);
        let expected: HashSet<_> = expected_heads.iter().collect();
        let actual: HashSet<_> = hs.iter().map(|h| &h.id).collect();
        if actual.is_empty() || expected != actual {
            bail!("便签版本已变化，请重新打开冲突处理");
        }
        let created_at = hs[0].created_at.clone();
        let r = Revision {
            schema: 1,
            id: id(),
            note_id: note_id.into(),
            parents: expected_heads,
            content,
            created_at,
            updated_at: now(),
        };
        r.validate()?;
        let tx = self.conn.transaction()?;
        let draft: Option<String> = tx
            .query_row("SELECT body FROM drafts WHERE note_id=?", [note_id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(d) = draft {
            let d: Revision = serde_json::from_str(&d)?;
            tx.execute(
                "INSERT INTO revisions(id,note_id,body) VALUES(?,?,?)",
                params![d.id, note_id, serde_json::to_string(&d)?],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO drafts(note_id,body) VALUES(?,?)",
            params![note_id, serde_json::to_string(&r)?],
        )?;
        tx.commit()?;
        self.note(note_id)
    }

    pub fn seal_drafts(&mut self) -> Result<()> {
        let drafts: Vec<String> = {
            let mut stmt = self.conn.prepare("SELECT body FROM drafts")?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        let tx = self.conn.transaction()?;
        for body in drafts {
            let r: Revision = serde_json::from_str(&body)?;
            tx.execute(
                "INSERT INTO revisions(id,note_id,body) VALUES(?,?,?)",
                params![r.id, r.note_id, body],
            )?;
        }
        tx.execute("DELETE FROM drafts", [])?;
        tx.commit()?;
        Ok(())
    }

    /// Run after a complete download round, never while ancestry is still arriving.
    pub fn auto_merge(&mut self) -> Result<usize> {
        let mut merged = Vec::new();
        for note in self.list()?.into_iter().filter(|n| !n.conflicts.is_empty()) {
            let mut stmt = self.conn.prepare("SELECT body FROM revisions WHERE note_id=?1 UNION ALL SELECT body FROM drafts WHERE note_id=?1")?;
            let rows = stmt.query_map([&note.id], |row| row.get::<_, String>(0))?;
            let rs = rows
                .map(|row| Ok(serde_json::from_str(&row?)?))
                .collect::<Result<Vec<Revision>>>()?;
            if let Some(r) = crate::merge::merge(&rs, &heads(&rs)) {
                merged.push(r);
            }
        }
        if merged.is_empty() {
            return Ok(0);
        }
        // Freeze source drafts before adding their merge; original branch contents survive.
        self.seal_drafts()?;
        self.insert_batch(&merged, false)?;
        Ok(merged.len())
    }

    /// Snapshot the current draft when opening history; keystrokes still coalesce normally.
    pub fn history(&mut self, note_id: &str, offset: usize) -> Result<Vec<Revision>> {
        self.note(note_id)?;
        if offset == 0 {
            let tx = self.conn.transaction()?;
            let draft: Option<String> = tx
                .query_row(
                    "SELECT body FROM drafts WHERE note_id=?",
                    [note_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(body) = draft {
                let r: Revision = serde_json::from_str(&body)?;
                tx.execute(
                    "INSERT INTO revisions(id,note_id,body) VALUES(?,?,?)",
                    params![r.id, note_id, body],
                )?;
                tx.execute("DELETE FROM drafts WHERE note_id=?", [note_id])?;
            }
            tx.commit()?;
        }
        let mut stmt = self.conn.prepare(
            "SELECT body FROM revisions WHERE note_id=? ORDER BY rowid DESC LIMIT 50 OFFSET ?",
        )?;
        let rows = stmt.query_map(params![note_id, i64::try_from(offset)?], |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn restore_revision(
        &mut self,
        note_id: &str,
        revision_id: &str,
        expected: &str,
    ) -> Result<Note> {
        let current = self.note(note_id)?;
        if current.head_id != expected || !current.conflicts.is_empty() {
            bail!("便签已变化或存在冲突，请重新打开历史并先处理冲突");
        }
        let body: String = self
            .conn
            .query_row(
                "SELECT body FROM revisions WHERE id=? AND note_id=?",
                params![revision_id, note_id],
                |row| row.get(0),
            )
            .context("历史版本不存在")?;
        let old: Revision = serde_json::from_str(&body)?;
        // Restoration is a new descendant, never a rewind or replacement of history.
        let note = self.resolve(note_id, vec![expected.to_owned()], old.content)?;
        self.history(note_id, 0)?;
        Ok(note)
    }

    pub fn pending(&self) -> Result<Vec<Revision>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body FROM revisions WHERE uploaded=0 ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    /// Compare lightweight IDs first; deserialize only this round's upload batch.
    pub fn upload_batch(
        &mut self,
        remote: &HashSet<String>,
        limit: usize,
    ) -> Result<(Vec<Revision>, usize)> {
        let pending: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM revisions WHERE uploaded=0")?;
            let rows = stmt
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        let tx = self.conn.transaction()?;
        for id in pending.iter().filter(|id| remote.contains(*id)) {
            tx.execute("UPDATE revisions SET uploaded=1 WHERE id=?", [id])?;
        }
        tx.commit()?;
        // Include previously uploaded objects if they have disappeared from the server.
        let mut missing: Vec<_> = self.known_ids()?.difference(remote).cloned().collect();
        missing.sort();
        let mut stmt = self.conn.prepare("SELECT body FROM revisions WHERE id=?")?;
        let batch = missing
            .iter()
            .take(limit)
            .map(|id| {
                let body: String = stmt.query_row([id], |row| row.get(0))?;
                Ok(serde_json::from_str(&body)?)
            })
            .collect::<Result<_>>()?;
        Ok((batch, missing.len()))
    }

    pub fn known_ids(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT id FROM revisions")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    pub fn mark_uploaded(&mut self, revision_id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE revisions SET uploaded=1 WHERE id=?", [revision_id])?;
        Ok(())
    }

    pub fn receive(&mut self, revision: &Revision) -> Result<()> {
        self.insert_batch(std::slice::from_ref(revision), true)
    }

    fn insert_batch(&mut self, revisions: &[Revision], uploaded: bool) -> Result<()> {
        for r in revisions {
            r.validate()?;
        }
        let tx = self.conn.transaction()?;
        for r in revisions {
            let existing: Option<String> = tx
                .query_row("SELECT body FROM revisions WHERE id=?", [&r.id], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(body) = existing {
                if serde_json::from_str::<Revision>(&body)? != *r {
                    bail!("检测到同一版本 ID 的不同内容，已停止同步以保护数据");
                }
                if uploaded {
                    tx.execute("UPDATE revisions SET uploaded=1 WHERE id=?", [&r.id])?;
                }
            } else {
                for parent in &r.parents {
                    let parent_note: Option<String> = tx
                        .query_row("SELECT note_id FROM revisions WHERE id=?", [parent], |r| {
                            r.get(0)
                        })
                        .optional()?;
                    if parent_note.is_some_and(|n| n != r.note_id) {
                        bail!("版本引用了其他便签");
                    }
                }
                tx.execute(
                    "INSERT INTO revisions(id,note_id,body,uploaded) VALUES(?,?,?,?)",
                    params![r.id, r.note_id, serde_json::to_string(r)?, uploaded],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn export(&mut self) -> Result<String> {
        self.seal_drafts()?;
        Ok(serde_json::to_string_pretty(&Backup {
            format: "qingnote-backup".into(),
            version: 1,
            revisions: self.all()?,
        })?)
    }

    pub fn import(&mut self, json: &str) -> Result<usize> {
        if json.len() > 32 * 1024 * 1024 {
            bail!("备份文件超过 32 MB，无法导入");
        }
        let backup: Backup = serde_json::from_str(json).context("不是有效的haonote JSON 备份")?;
        if backup.format != "qingnote-backup" || backup.version != 1 {
            bail!("不支持的备份版本");
        }
        if backup.revisions.len() > 100_000 {
            bail!("备份中的版本数量过多");
        }
        // Validate completely before changing local data. Import merges, never replaces.
        for r in &backup.revisions {
            r.validate()?;
        }
        self.seal_drafts()?;
        let before = self.known_ids()?.len();
        self.insert_batch(&backup.revisions, false)?;
        Ok(self.known_ids()?.len() - before)
    }

    pub fn import_sticky_notes(&mut self, path: &Path) -> Result<usize> {
        let existing: HashSet<_> = self.all()?.into_iter().map(|r| r.note_id).collect();
        let revisions: Vec<_> = crate::sticky_import::read(path)?
            .into_iter()
            .filter(|r| !existing.contains(&r.note_id))
            .collect();
        // One atomic merge. Never re-import a note already migrated, even if it
        // was edited/deleted here or changed in a later source backup.
        self.insert_batch(&revisions, false)?;
        Ok(revisions.len())
    }

    pub fn setting<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(value.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn set_setting<T: Serialize>(&mut self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    /// Once bound, changing accounts/folders is refused: otherwise notes could leak to another account.
    pub fn bind_sync(&mut self, config: &DavConfig) -> Result<()> {
        config.validate()?;
        if let Some(previous) = self.setting::<DavConfig>("sync_config")? {
            if previous != *config {
                bail!("此资料库已绑定同步目录。为防止混入其他账号，请继续使用原账号和目录；更换设备可导出备份");
            }
        }
        self.set_setting("sync_config", config)
    }

    pub fn take_request_budget(&mut self) -> Result<()> {
        let time = chrono::Utc::now().timestamp();
        let mut timestamps = self
            .setting::<Vec<i64>>("request_times")?
            .unwrap_or_default();
        timestamps.retain(|t| *t > time - 1800);
        if timestamps.len() >= 150 {
            bail!("已达到本机同步请求预算，将在额度恢复后自动继续");
        }
        timestamps.push(time);
        self.set_setting("request_times", &timestamps)
    }
}

fn heads(revisions: &[Revision]) -> Vec<&Revision> {
    let parents: HashSet<_> = revisions.iter().flat_map(|r| r.parents.iter()).collect();
    revisions
        .iter()
        .filter(|r| !parents.contains(&r.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Store {
        Store::open(":memory:").unwrap()
    }
    fn text(s: &str) -> Content {
        Content {
            text: s.into(),
            ..Content::default()
        }
    }
    fn share(a: &mut Store, b: &mut Store) {
        a.seal_drafts().unwrap();
        for r in a.pending().unwrap() {
            b.receive(&r).unwrap();
            a.mark_uploaded(&r.id).unwrap();
        }
    }

    #[test]
    fn indexed_heads_match_full_history_with_out_of_order_arrival_and_drafts() {
        let mut source = db();
        let mut note = source.save(None, None, text("base")).unwrap();
        source.seal_drafts().unwrap();
        for i in 0..8 {
            note = source
                .save(Some(&note.id), Some(&note.head_id), text(&i.to_string()))
                .unwrap();
            source.seal_drafts().unwrap();
        }
        let mut target = db();
        for revision in source.all().unwrap().iter().rev() {
            target.receive(revision).unwrap();
            let all = target.all().unwrap();
            let expected: HashSet<_> = heads(&all).iter().map(|r| r.id.clone()).collect();
            let actual: HashSet<_> = target
                .current_revisions(None)
                .unwrap()
                .iter()
                .map(|r| r.id.clone())
                .collect();
            assert_eq!(actual, expected);
        }
        let edited = target
            .save(Some(&note.id), Some(&note.head_id), text("draft"))
            .unwrap();
        assert_eq!(target.current_revisions(None).unwrap().len(), 1);
        assert_eq!(target.note(&note.id).unwrap().head_id, edited.head_id);
        target.seal_drafts().unwrap();
        assert_eq!(target.current_revisions(None).unwrap().len(), 1);
    }

    #[test]
    fn legacy_database_backfills_parent_index_without_changing_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let mut store = Store::open(&path).unwrap();
        let note = store.save(None, None, text("old")).unwrap();
        store.seal_drafts().unwrap();
        let latest = store
            .save(Some(&note.id), Some(&note.head_id), text("new"))
            .unwrap();
        let before = store.export().unwrap();
        store
            .conn
            .execute_batch("DROP TRIGGER index_revision_parents; DROP TABLE revision_parents;")
            .unwrap();
        drop(store);
        let mut reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.note(&note.id).unwrap().head_id, latest.head_id);
        assert_eq!(reopened.export().unwrap(), before);
        assert_eq!(reopened.current_revisions(None).unwrap().len(), 1);
    }

    #[test]
    fn long_history_reads_only_heads_and_bounded_upload_bodies() {
        let mut store = db();
        let note = store.save(None, None, text("seed")).unwrap();
        store.seal_drafts().unwrap();
        let mut previous = note.head_id.clone();
        let base = store.all().unwrap().remove(0);
        let revisions: Vec<_> = (0..1000)
            .map(|_| {
                let revision = Revision {
                    id: id(),
                    parents: vec![previous.clone()],
                    content: text(&"x".repeat(4096)),
                    ..base.clone()
                };
                previous = revision.id.clone();
                revision
            })
            .collect();
        store.insert_batch(&revisions, false).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            assert_eq!(store.list().unwrap().len(), 1);
        }
        let indexed = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            assert_eq!(heads(&store.all().unwrap()).len(), 1);
        }
        eprintln!(
            "1001 revisions / 20 reads: indexed={indexed:?}, full-history={:?}",
            start.elapsed()
        );
        assert_eq!(store.current_revisions(None).unwrap().len(), 1);
        let (batch, remaining) = store.upload_batch(&HashSet::new(), 12).unwrap();
        assert_eq!(batch.len(), 12);
        assert_eq!(remaining, 1001);
        let mut remote = store.known_ids().unwrap();
        assert!(store.upload_batch(&remote, 12).unwrap().0.is_empty());
        assert!(store.pending().unwrap().is_empty());
        // A cloud object removed after a successful upload must still be repaired.
        remote.remove(&previous);
        let (batch, remaining) = store.upload_batch(&remote, 12).unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(batch[0].id, previous);
        assert_eq!(store.history(&note.id, 0).unwrap().len(), 50);
        assert_eq!(store.all().unwrap().len(), 1001);
    }

    #[test]
    fn automatic_merge_converges_and_preserves_history() {
        let (mut a, mut b) = (db(), db());
        let note = a.save(None, None, text("first\nsecond\nthird\n")).unwrap();
        share(&mut a, &mut b);
        let left = a
            .save(
                Some(&note.id),
                Some(&note.head_id),
                text("FIRST\nsecond\nthird\n"),
            )
            .unwrap();
        let right = b
            .save(
                Some(&note.id),
                Some(&note.head_id),
                text("first\nsecond\nTHIRD\n"),
            )
            .unwrap();
        share(&mut a, &mut b);
        share(&mut b, &mut a);
        assert_eq!(a.auto_merge().unwrap(), 1);
        assert_eq!(b.auto_merge().unwrap(), 1);
        assert_eq!(a.auto_merge().unwrap(), 0);
        let merged = a.note(&note.id).unwrap();
        assert_eq!(merged.head_id, b.note(&note.id).unwrap().head_id);
        assert_eq!(merged.content.text, "FIRST\nsecond\nTHIRD\n");
        assert!(merged.conflicts.is_empty());
        share(&mut a, &mut b);
        let history = b.history(&note.id, 0).unwrap();
        assert_eq!(history.len(), 4);
        assert!(history.iter().any(|r| r.id == left.head_id));
        assert!(history.iter().any(|r| r.id == right.head_id));
        let restored = b
            .restore_revision(&note.id, &note.head_id, &merged.head_id)
            .unwrap();
        assert_eq!(restored.content, note.content);
        assert_ne!(restored.head_id, note.head_id);
        assert_eq!(b.history(&note.id, 0).unwrap().len(), 5);
        assert!(b
            .restore_revision(&note.id, &left.head_id, &merged.head_id)
            .is_err());
        let mut backup = db();
        backup.import(&b.export().unwrap()).unwrap();
        assert_eq!(backup.history(&note.id, 0).unwrap().len(), 5);
        assert_eq!(backup.note(&note.id).unwrap().content, note.content);
    }

    #[test]
    fn automatic_merge_leaves_overlapping_changes_for_user() {
        let (mut a, mut b) = (db(), db());
        let note = a.save(None, None, text("base")).unwrap();
        share(&mut a, &mut b);
        a.save(Some(&note.id), Some(&note.head_id), text("left"))
            .unwrap();
        b.save(Some(&note.id), Some(&note.head_id), text("right"))
            .unwrap();
        share(&mut a, &mut b);
        assert_eq!(b.auto_merge().unwrap(), 0);
        assert_eq!(b.note(&note.id).unwrap().conflicts.len(), 1);
        let head = b.note(&note.id).unwrap().head_id;
        assert!(b.restore_revision(&note.id, &note.head_id, &head).is_err());
        assert_eq!(b.history(&note.id, 0).unwrap().len(), 3);
    }

    #[test]
    fn independent_title_survives_sync_conflict_and_backup() {
        let (mut a, mut b) = (db(), db());
        let note = a.save(None, None, text("unchanged body")).unwrap();
        share(&mut a, &mut b);
        let mut first = note.content.clone();
        first.title = "电脑标题".into();
        a.save(Some(&note.id), Some(&note.head_id), first).unwrap();
        let mut second = note.content.clone();
        second.title = "手机标题".into();
        b.save(Some(&note.id), Some(&note.head_id), second).unwrap();
        share(&mut a, &mut b);
        let merged = b.note(&note.id).unwrap();
        assert_eq!(merged.content.text, "unchanged body");
        assert_eq!(merged.conflicts.len(), 1);
        assert_ne!(merged.content.title, merged.conflicts[0].content.title);
        let backup = b.export().unwrap();
        let mut restored = db();
        restored.import(&backup).unwrap();
        assert_eq!(restored.note(&note.id).unwrap().content, merged.content);
    }

    #[test]
    fn offline_edits_coalesce_and_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.db");
        let mut a = Store::open(&path).unwrap();
        let mut n = a.save(None, None, text("first")).unwrap();
        for i in 0..30 {
            n = a
                .save(Some(&n.id), Some(&n.head_id), text(&i.to_string()))
                .unwrap();
        }
        assert_eq!(a.all().unwrap().len(), 1);
        drop(a);
        let a = Store::open(&path).unwrap();
        assert_eq!(a.note(&n.id).unwrap().content.text, "29");
    }

    #[test]
    fn concurrent_edits_preserved_then_explicitly_resolved() {
        let (mut a, mut b) = (db(), db());
        let n = a.save(None, None, text("base")).unwrap();
        share(&mut a, &mut b);
        let a_note = a
            .save(Some(&n.id), Some(&n.head_id), text("computer"))
            .unwrap();
        let b_note = b
            .save(Some(&n.id), Some(&n.head_id), text("phone"))
            .unwrap();
        share(&mut a, &mut b);
        share(&mut b, &mut a);
        let n = a.note(&n.id).unwrap();
        assert_eq!(n.conflicts.len(), 1);
        let merged = a
            .resolve(
                &n.id,
                vec![a_note.head_id, b_note.head_id],
                text("computer + phone"),
            )
            .unwrap();
        assert!(merged.conflicts.is_empty());
        share(&mut a, &mut b);
        assert_eq!(b.note(&n.id).unwrap().content.text, "computer + phone");
        assert!(b.note(&n.id).unwrap().conflicts.is_empty());
    }

    #[test]
    fn deleting_and_offline_editing_still_keeps_both_versions() {
        let (mut a, mut b) = (db(), db());
        let n = a.save(None, None, text("base")).unwrap();
        share(&mut a, &mut b);
        a.save(
            Some(&n.id),
            Some(&n.head_id),
            Content {
                deleted: true,
                ..text("base")
            },
        )
        .unwrap();
        b.save(Some(&n.id), Some(&n.head_id), text("offline"))
            .unwrap();
        share(&mut a, &mut b);
        share(&mut b, &mut a);
        assert_eq!(a.note(&n.id).unwrap().conflicts.len(), 1);
    }

    #[test]
    fn stale_window_cannot_overwrite_newer_local_draft() {
        let mut a = db();
        let n = a.save(None, None, text("one")).unwrap();
        a.save(Some(&n.id), Some(&n.head_id), text("two")).unwrap();
        assert!(a
            .save(Some(&n.id), Some(&n.head_id), text("stale"))
            .is_err());
    }

    #[test]
    fn edits_during_upload_do_not_change_sealed_revision() {
        let mut a = db();
        let n = a.save(None, None, text("one")).unwrap();
        a.seal_drafts().unwrap();
        a.save(Some(&n.id), Some(&n.head_id), text("two")).unwrap();
        a.mark_uploaded(&n.head_id).unwrap();
        assert_eq!(a.note(&n.id).unwrap().content.text, "two");
        assert!(a.note(&n.id).unwrap().pending);
        assert_eq!(a.all().unwrap().len(), 2);
    }

    #[test]
    fn backup_merge_is_idempotent_and_atomic_on_invalid_data() {
        let (mut a, mut b) = (db(), db());
        a.save(None, None, text("keep")).unwrap();
        let backup = a.export().unwrap();
        assert_eq!(b.import(&backup).unwrap(), 1);
        assert_eq!(b.import(&backup).unwrap(), 0);
        let corrupt = backup.replace("butter", "unknown");
        assert!(b.import(&corrupt).is_err());
        assert_eq!(b.list().unwrap().len(), 1);
    }

    #[test]
    fn repeated_network_revision_is_safe_but_mutation_is_rejected() {
        let (mut a, mut b) = (db(), db());
        a.save(None, None, text("one")).unwrap();
        a.seal_drafts().unwrap();
        let mut r = a.pending().unwrap().remove(0);
        b.receive(&r).unwrap();
        b.receive(&r).unwrap();
        r.content.text = "tampered".into();
        assert!(b.receive(&r).is_err());
        assert_eq!(b.list().unwrap()[0].content.text, "one");
    }

    #[test]
    fn saving_keeps_edited_branch_when_remote_clock_is_ahead() {
        let (mut a, mut b) = (db(), db());
        let n = a.save(None, None, text("base")).unwrap();
        share(&mut a, &mut b);
        let local = a
            .save(Some(&n.id), Some(&n.head_id), text("local"))
            .unwrap();
        b.save(Some(&n.id), Some(&n.head_id), text("remote"))
            .unwrap();
        b.seal_drafts().unwrap();
        let mut remote = b.pending().unwrap().remove(0);
        remote.updated_at = "2099-01-01T00:00:00.000Z".into();
        a.receive(&remote).unwrap();
        let saved = a
            .save(Some(&n.id), Some(&local.head_id), text("local continued"))
            .unwrap();
        assert_eq!(saved.content.text, "local continued");
        assert_eq!(saved.conflicts.len(), 1);
        assert_eq!(saved.conflicts[0].content.text, "remote");
        let next = a
            .save(Some(&n.id), Some(&saved.head_id), text("local again"))
            .unwrap();
        assert_eq!(next.conflicts[0].id, remote.id);
    }

    #[test]
    fn an_unedited_offline_device_cannot_resurrect_a_deleted_note() {
        let (mut a, mut b) = (db(), db());
        let n = a.save(None, None, text("base")).unwrap();
        share(&mut a, &mut b);
        a.save(
            Some(&n.id),
            Some(&n.head_id),
            Content {
                deleted: true,
                ..text("base")
            },
        )
        .unwrap();
        share(&mut b, &mut a);
        share(&mut a, &mut b);
        assert!(b.note(&n.id).unwrap().content.deleted);
        assert!(b.note(&n.id).unwrap().conflicts.is_empty());
    }

    #[test]
    fn request_budget_survives_restart_and_rejects_extra_requests() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.db");
        let mut a = Store::open(&path).unwrap();
        for _ in 0..150 {
            a.take_request_budget().unwrap();
        }
        drop(a);
        let mut a = Store::open(&path).unwrap();
        assert!(a.take_request_budget().is_err());
    }
}
