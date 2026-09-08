//! Read-only, one-time migration of Simple Sticky Notes backups, not live sync.
use crate::{Content, Revision, MAX_TEXT_BYTES};
use anyhow::{bail, Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::{Connection, OpenFlags};
use std::{collections::HashSet, path::Path};
use uuid::Uuid;

fn timestamp(days: f64) -> Result<String> {
    // Simple Sticky Notes uses Delphi/OLE dates without a timezone. Keep the
    // original wall-clock time in UTC for deterministic imports across devices.
    if !days.is_finite() || !(2.0..=2_958_465.0).contains(&days) {
        bail!("便签日期超出支持范围");
    }
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    Ok(epoch
        .checked_add_signed(Duration::milliseconds((days * 86_400_000.0).round() as i64))
        .context("无效的便签日期")?
        .and_utc()
        .to_rfc3339())
}

fn color(value: i64) -> &'static str {
    // Windows COLORREF stores red in the least significant byte.
    let rgb = [value & 255, (value >> 8) & 255, (value >> 16) & 255];
    [
        ("butter", [255, 244, 173]),
        ("sage", [220, 232, 200]),
        ("sky", [210, 232, 245]),
        ("rose", [245, 214, 219]),
        ("lilac", [229, 216, 239]),
        ("paper", [240, 239, 230]),
    ]
    .into_iter()
    .min_by_key(|(_, candidate)| {
        rgb.iter()
            .zip(candidate)
            .map(|(a, b)| (a - b).pow(2))
            .sum::<i64>()
    })
    .unwrap()
    .0
}

pub fn read(path: &Path) -> Result<Vec<Revision>> {
    if std::fs::metadata(path)?.len() > 32 * 1024 * 1024 {
        bail!("Simple Sticky Notes 备份超过 32 MB");
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; BEGIN;")?;
    let table: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE upper(name)='NOTES' AND type='table'",
        [],
        |r| r.get(0),
    )?;
    if table != 1 {
        bail!("不是受支持的 Simple Sticky Notes 数据库");
    }
    let count: i64 = conn.query_row("SELECT count(*) FROM NOTES", [], |r| r.get(0))?;
    if count > 10_000 {
        bail!("一次最多导入 10000 条便签");
    }
    let mut stmt = conn
        .prepare("SELECT ID,STATE,CREATED,UPDATED,DELETED,COLOR,TYPE,TITLE,TEXT FROM NOTES")
        .context("Simple Sticky Notes 表结构不受支持")?;
    let mut rows = stmt.query([])?;
    let mut notes = Vec::new();
    let mut ids = HashSet::new();
    let mut bytes = 0;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let state: i64 = row.get(1)?;
        let created: f64 = row.get(2)?;
        let updated: f64 = row.get(3)?;
        let deleted: f64 = row.get(4)?;
        let kind: i64 = row.get(6)?;
        if ![0, 1].contains(&state) || kind != 0 || !deleted.is_finite() || deleted < 0.0 {
            bail!("便签 {id} 使用不支持的状态或内容类型，未导入任何便签");
        }
        let text: String = row.get(8).context("缺少纯文本正文，无法安全导入")?;
        bytes += text.len();
        if text.len() > MAX_TEXT_BYTES || bytes > 32 * 1024 * 1024 {
            bail!("便签文本超过导入限制");
        }
        let title: String = row.get::<_, Option<String>>(7)?.unwrap_or_default();
        let note_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("haonote:SimpleStickyNotes:{id}:{:x}", created.to_bits()).as_bytes(),
        );
        if !ids.insert(note_id) {
            bail!("数据库中存在重复便签标识");
        }
        let mut revision = Revision {
            schema: 1,
            id: String::new(),
            note_id: note_id.to_string(),
            parents: vec![],
            content: Content {
                text: text.replace("\r\n", "\n"),
                title,
                color: color(row.get(5)?).into(),
                archived: false,
                deleted: state == 0 || deleted > 0.0,
            },
            created_at: timestamp(created)?,
            updated_at: timestamp(updated)?,
        };
        // Different source snapshots imported on different devices must become
        // separate conflict heads, never different bodies with the same ID.
        revision.id = Uuid::new_v5(&note_id, &serde_json::to_vec(&revision)?).to_string();
        revision
            .validate()
            .with_context(|| format!("便签 {id} 无法导入"))?;
        notes.push(revision);
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    fn fixture(path: &Path) -> Connection {
        let db = Connection::open(path).unwrap();
        db.execute_batch("CREATE TABLE NOTES(ID INTEGER,STATE INTEGER,CREATED FLOAT,UPDATED FLOAT,DELETED FLOAT,COLOR INTEGER,TYPE INTEGER,TITLE TEXT,TEXT TEXT);
            INSERT INTO NOTES VALUES(1,1,46000,46001,0,10092543,0,'测试标题','中文正文');
            INSERT INTO NOTES VALUES(2,0,46000,46002,46002,12632256,0,'回收站','删除的正文');").unwrap();
        db
    }

    #[test]
    fn imports_read_only_and_preserves_edits_on_reimport() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Notes.db");
        let source = fixture(&path);
        let before = std::fs::read(&path).unwrap();
        let mut store = Store::open(dir.path().join("haonote.db")).unwrap();
        assert_eq!(store.import_sticky_notes(&path).unwrap(), 2);
        assert_eq!(store.import_sticky_notes(&path).unwrap(), 0);
        let notes = store.list().unwrap();
        assert_eq!(notes.iter().filter(|n| n.content.deleted).count(), 1);
        let note = notes.iter().find(|n| !n.content.deleted).unwrap();
        assert_eq!(note.content.title, "测试标题");
        let mut content = note.content.clone();
        content.text = "本地修改".into();
        store
            .save(Some(&note.id), Some(&note.head_id), content)
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        source
            .execute("UPDATE NOTES SET TEXT='来源更新' WHERE ID=1", [])
            .unwrap();
        let changed_source = read(&path).unwrap();
        let changed = changed_source
            .iter()
            .find(|r| r.note_id == note.id)
            .unwrap();
        assert_ne!(changed.id, note.head_id);
        assert_eq!(store.import_sticky_notes(&path).unwrap(), 0);
        assert_eq!(store.note(&note.id).unwrap().content.text, "本地修改");
    }

    #[test]
    fn invalid_row_never_partially_imports() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Notes.db");
        let source = fixture(&path);
        source
            .execute("UPDATE NOTES SET TYPE=99 WHERE ID=2", [])
            .unwrap();
        let mut store = Store::open(dir.path().join("haonote.db")).unwrap();
        assert!(store.import_sticky_notes(&path).is_err());
        assert!(store.list().unwrap().is_empty());
        assert!(timestamp(f64::NAN).is_err());
    }

    #[test]
    #[ignore = "Optional local sample check; never commit personal databases"]
    fn local_sample() {
        let path = std::env::var("STICKY_SAMPLE_DB").unwrap();
        let before = std::fs::read(&path).unwrap();
        let revisions = read(Path::new(&path)).unwrap();
        assert!(!revisions.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().join("import.db")).unwrap();
        assert_eq!(
            store.import_sticky_notes(Path::new(&path)).unwrap(),
            revisions.len()
        );
        assert_eq!(store.import_sticky_notes(Path::new(&path)).unwrap(), 0);
        assert_eq!(store.list().unwrap().len(), revisions.len());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        println!(
            "Validated {} notes without displaying their contents",
            revisions.len()
        );
    }
}
