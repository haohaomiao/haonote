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
        let mut grouped: BTreeMap<String, Vec<Revision>> = BTreeMap::new();
        for r in self.all()? {
            grouped.entry(r.note_id.clone()).or_default().push(r);
        }
        let mut stmt = self.conn.prepare(
            "SELECT note_id FROM revisions WHERE uploaded=0 UNION SELECT note_id FROM drafts",
        )?;
        let pending: HashSet<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
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
                    pending: pending.contains(&id),
                });
            }
        }
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        Ok(notes)
    }

    pub fn note(&self, note_id: &str) -> Result<Note> {
        self.list()?
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
        let rs: Vec<_> = self
            .all()?
            .into_iter()
            .filter(|r| r.note_id == note_id)
            .collect();
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
        let revisions: Vec<_> = self
            .all()?
            .into_iter()
            .filter(|r| r.note_id == revision.note_id)
            .collect();
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
        let rs: Vec<_> = self
            .all()?
            .into_iter()
            .filter(|r| r.note_id == note_id)
            .collect();
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

    pub fn pending(&self) -> Result<Vec<Revision>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body FROM revisions WHERE uploaded=0 ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
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
