use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_TEXT_BYTES: usize = 128 * 1024;
pub const COLORS: [&str; 6] = ["butter", "sage", "sky", "rose", "lilac", "paper"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Content {
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub color: String,
    pub archived: bool,
    pub deleted: bool,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            text: String::new(),
            title: String::new(),
            color: "butter".into(),
            archived: false,
            deleted: false,
        }
    }
}

impl Content {
    pub fn validate(&self) -> Result<()> {
        if self.title.chars().count() > 160 || self.title.contains(['\r', '\n']) {
            bail!("标题最多 160 个字符，不能换行");
        }
        if self.text.len() > MAX_TEXT_BYTES {
            bail!("一张便签最多支持 128 KB 文本");
        }
        if !COLORS.contains(&self.color.as_str()) {
            bail!("未知便签颜色");
        }
        Ok(())
    }
}

/// Immutable, device-independent revision. Multiple heads retain concurrent edits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    pub schema: u8,
    pub id: String,
    pub note_id: String,
    pub parents: Vec<String>,
    pub content: Content,
    pub created_at: String,
    pub updated_at: String,
}

impl Revision {
    pub fn validate(&self) -> Result<()> {
        if self.schema != 1 {
            bail!("云端数据格式需要更新版本的轻笺");
        }
        Uuid::parse_str(&self.id)?;
        Uuid::parse_str(&self.note_id)?;
        if self.parents.len() > 64 {
            bail!("便签分支过多，请先处理冲突");
        }
        let mut unique = std::collections::HashSet::new();
        for p in &self.parents {
            Uuid::parse_str(p)?;
            if p == &self.id || !unique.insert(p) {
                bail!("无效的版本关系");
            }
        }
        chrono::DateTime::parse_from_rfc3339(&self.created_at)?;
        chrono::DateTime::parse_from_rfc3339(&self.updated_at)?;
        self.content.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub head_id: String,
    pub content: Content,
    pub created_at: String,
    pub updated_at: String,
    pub conflicts: Vec<Revision>,
    pub pending: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub running: bool,
    pub last_success: Option<String>,
    pub message: String,
    pub pending: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DavConfig {
    pub url: String,
    pub username: String,
    pub folder: String,
}

impl DavConfig {
    pub fn validate(&self) -> Result<url::Url> {
        let mut url = url::Url::parse(self.url.trim())?;
        if url.scheme() != "https" {
            bail!("同步地址必须使用 HTTPS");
        }
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("请输入不含用户名、密码或查询参数的 WebDAV 地址");
        }
        if self.username.trim().is_empty() {
            bail!("请填写 WebDAV 账号");
        }
        if self.folder.is_empty()
            || self.folder.len() > 64
            || !self
                .folder
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("同步目录只能包含英文字母、数字、短横线和下划线");
        }
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(url)
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_content_and_title_limits() {
        let json = r#"{"text":"body","color":"butter","archived":false,"deleted":false}"#;
        let mut content: Content = serde_json::from_str(json).unwrap();
        assert!(content.title.is_empty());
        assert_eq!(serde_json::to_string(&content).unwrap(), json);
        content.title = "独立标题".into();
        assert!(content.validate().is_ok());
        content.title = "x".repeat(161);
        assert!(content.validate().is_err());
        content.title = "line\nbreak".into();
        assert!(content.validate().is_err());
    }
    #[test]
    fn credentials_and_unencrypted_or_ambiguous_urls_are_rejected() {
        for url in [
            "http://example.com/dav/",
            "https://user:password@example.com/",
            "https://example.com/?token=abc",
            "https://example.com/#fragment",
        ] {
            let config = DavConfig {
                url: url.into(),
                username: "user".into(),
                folder: "QingNote".into(),
            };
            assert!(config.validate().is_err());
        }
        let config = DavConfig {
            url: "https://example.com/dav".into(),
            username: "user".into(),
            folder: "../other".into(),
        };
        assert!(config.validate().is_err());
    }
}
