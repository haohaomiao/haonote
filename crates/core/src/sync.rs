//! Append-only WebDAV revisions avoid relying on provider-specific locks or ETags.
//! Unsent edits coalesce locally; once sealed, an object is never modified.
use crate::{model::*, Store};
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use reqwest::{Client, Method, Response, StatusCode};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use url::Url;

pub type SharedStore = Arc<Mutex<Store>>;
const MAX_OBJECT: usize = 1024 * 1024;
const MAX_LISTING: usize = 16 * 1024 * 1024;

pub struct Dav {
    client: Client,
    base: Url,
    root: Url,
    username: String,
    password: String,
    store: SharedStore,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub uploaded: usize,
    pub downloaded: usize,
    pub remaining: usize,
}

impl Dav {
    pub fn new(config: &DavConfig, password: String, store: SharedStore) -> Result<Self> {
        let base = config.validate()?.join(&format!("{}/", config.folder))?;
        Self::with_base(base, &config.username, password, store)
    }

    fn with_base(base: Url, username: &str, password: String, store: SharedStore) -> Result<Self> {
        if password.is_empty() {
            bail!("请填写第三方应用密码");
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("haonote/0.1.5")
            .build()?;
        let root = base.join("v1/")?;
        Ok(Self {
            client,
            base,
            root,
            username: username.into(),
            password,
            store,
        })
    }

    async fn request(&self, method: Method, url: Url, body: Option<String>) -> Result<Response> {
        self.store
            .lock()
            .map_err(|_| anyhow::anyhow!("本地资料库繁忙"))?
            .take_request_budget()?;
        let mut request = self
            .client
            .request(method.clone(), url)
            .basic_auth(&self.username, Some(&self.password));
        if method.as_str() == "PROPFIND" {
            request = request
                .header("Depth", "1")
                .header("Content-Type", "application/xml; charset=utf-8");
        }
        if method == Method::PUT {
            request = request
                .header("Content-Type", "application/json")
                .header("If-None-Match", "*");
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send().await.map_err(|error| {
            if error.is_timeout() {
                anyhow::anyhow!("连接超时，内容已保存在本机，稍后会重试")
            } else {
                anyhow::anyhow!("无法连接 WebDAV，请检查网络、地址与证书")
            }
        })
    }

    async fn ensure_folders(&self) -> Result<()> {
        for url in [&self.base, &self.root] {
            let response = self
                .request(Method::from_bytes(b"MKCOL")?, url.clone(), None)
                .await?;
            if response.status() != StatusCode::METHOD_NOT_ALLOWED {
                check(response.status())?;
            }
        }
        Ok(())
    }

    pub async fn test_connection(&self) -> Result<()> {
        self.ensure_folders().await?;
        self.list().await?;
        // Only this unique temporary file is removed; user notes are never deleted remotely.
        let url = self.root.join(&format!("probe-{}.json", id()))?;
        let marker = "{\"qingnote\":\"connection-test\"}";
        check(
            self.request(Method::PUT, url.clone(), Some(marker.into()))
                .await?
                .status(),
        )?;
        let result = async {
            let response = self.request(Method::GET, url.clone(), None).await?;
            check(response.status())?;
            if read_limited(response, 1024).await? != marker {
                bail!("WebDAV 读写校验失败");
            }
            Ok::<(), anyhow::Error>(())
        }
        .await;
        let cleanup = self.request(Method::DELETE, url, None).await?;
        check(cleanup.status()).context("测试文件清理失败，请检查 WebDAV 删除权限")?;
        result
    }

    async fn list(&self) -> Result<HashSet<String>> {
        let body = "<?xml version=\"1.0\"?><d:propfind xmlns:d=\"DAV:\"><d:prop><d:resourcetype/></d:prop></d:propfind>";
        let response = self
            .request(
                Method::from_bytes(b"PROPFIND")?,
                self.root.clone(),
                Some(body.into()),
            )
            .await?;
        if response.status() != StatusCode::MULTI_STATUS {
            check(response.status())?;
            bail!("服务未返回有效的 WebDAV 目录列表");
        }
        let xml = read_limited(response, MAX_LISTING).await?;
        parse_listing(&xml, &self.root)
    }

    pub async fn sync(&self) -> Result<Outcome> {
        // Avoid MKCOL on every poll. Recreate directories only when absent.
        let remote = match self.list().await {
            Ok(ids) => ids,
            Err(error) if error.to_string().contains("HTTP 404") => {
                self.ensure_folders().await?;
                self.list().await?
            }
            Err(error) => return Err(error),
        };
        let known = self.store.lock().unwrap().known_ids()?;
        let mut unknown: Vec<_> = remote.difference(&known).cloned().collect();
        unknown.sort();
        let mut outcome = Outcome::default();
        // Bound each round; periodic rounds resume using the persisted local IDs.
        for revision_id in unknown.iter().take(24) {
            let response = self
                .request(
                    Method::GET,
                    self.root.join(&format!("{revision_id}.json"))?,
                    None,
                )
                .await?;
            check(response.status())?;
            let r: Revision = serde_json::from_str(&read_limited(response, MAX_OBJECT).await?)
                .context("云端存在无效的便签文件，已停止本轮同步")?;
            if r.id != *revision_id {
                bail!("云端文件名与版本 ID 不符");
            }
            self.store.lock().unwrap().receive(&r)?;
            outcome.downloaded += 1;
        }
        let local = {
            let mut store = self.store.lock().unwrap();
            store.seal_drafts()?;
            store.all()?
        };
        let missing: Vec<_> = local.iter().filter(|r| !remote.contains(&r.id)).collect();
        for r in missing.iter().take(12) {
            let url = self.root.join(&format!("{}.json", r.id))?;
            let response = self
                .request(Method::PUT, url.clone(), Some(serde_json::to_string(r)?))
                .await?;
            if response.status() == StatusCode::PRECONDITION_FAILED {
                // A retry after an ambiguous network failure must verify the existing object.
                let response = self.request(Method::GET, url, None).await?;
                check(response.status())?;
                let existing: Revision =
                    serde_json::from_str(&read_limited(response, MAX_OBJECT).await?)?;
                if existing != **r {
                    bail!("云端同名版本内容不一致，已停止上传");
                }
            } else {
                check(response.status())?;
            }
            self.store.lock().unwrap().mark_uploaded(&r.id)?;
            outcome.uploaded += 1;
        }
        {
            let mut store = self.store.lock().unwrap();
            for r in &local {
                if remote.contains(&r.id) {
                    store.mark_uploaded(&r.id)?;
                }
            }
        }
        outcome.remaining = unknown.len().saturating_sub(outcome.downloaded)
            + missing.len().saturating_sub(outcome.uploaded);
        Ok(outcome)
    }
}

fn check(status: StatusCode) -> Result<()> {
    match status.as_u16() {
        200..=299 => Ok(()),
        401 | 403 => bail!("WebDAV 账号、应用密码或目录权限不正确"),
        429 => bail!("WebDAV 请求过于频繁，稍后将自动重试"),
        300..=399 => bail!("WebDAV 地址发生重定向，请填写最终的 HTTPS 地址"),
        code => bail!("WebDAV 请求失败（HTTP {code}），本地数据不受影响"),
    }
}

async fn read_limited(response: Response, max: usize) -> Result<String> {
    if response
        .content_length()
        .is_some_and(|len| len > max as u64)
    {
        bail!("云端响应过大");
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("下载中断，将在下次同步重试")?;
        if bytes.len() + chunk.len() > max {
            bail!("云端响应过大");
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).context("云端内容不是 UTF-8 文本")
}

fn parse_listing(xml: &str, root: &Url) -> Result<HashSet<String>> {
    let doc = roxmltree::Document::parse(xml).context("WebDAV 返回了无效的 XML")?;
    if !doc.root_element().has_tag_name(("DAV:", "multistatus")) {
        bail!("无效的 WebDAV 多状态响应");
    }
    let mut ids = HashSet::new();
    for node in doc
        .descendants()
        .filter(|n| n.has_tag_name(("DAV:", "href")))
    {
        let Some(href) = node.text() else { continue };
        let url = root.join(href)?;
        if url.origin() != root.origin() || url.query().is_some() || url.fragment().is_some() {
            continue;
        }
        let Some(name) = url.path().strip_prefix(root.path()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if uuid::Uuid::parse_str(id).is_ok() && id.len() == 36 && !id.contains('/') {
            ids.insert(id.into());
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicBool, Ordering},
        thread,
    };
    use tiny_http::{Header, Response as Reply, Server};

    struct MockDav {
        base: Url,
        files: Arc<Mutex<BTreeMap<String, String>>>,
        fail_put: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl MockDav {
        fn start() -> Self {
            let server = Server::http("127.0.0.1:0").unwrap();
            let base = Url::parse(&format!("http://{}/QingNote/", server.server_addr())).unwrap();
            let files = Arc::new(Mutex::new(BTreeMap::<String, String>::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let fail_put = Arc::new(AtomicBool::new(false));
            let (f, s, fail) = (files.clone(), stop.clone(), fail_put.clone());
            let worker = thread::spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    let Some(mut request) = server.recv_timeout(Duration::from_millis(50)).unwrap()
                    else {
                        continue;
                    };
                    let path = request.url().to_string();
                    let (code, body) = match request.method().as_str() {
                        "MKCOL" => (201, String::new()),
                        "PROPFIND" => {
                            let files = f.lock().unwrap();
                            let content = files
                                .keys()
                                .map(|p| format!("<d:response><d:href>{p}</d:href></d:response>"))
                                .collect::<String>();
                            (
                                207,
                                format!(
                                    "<d:multistatus xmlns:d=\"DAV:\">{content}</d:multistatus>"
                                ),
                            )
                        }
                        "PUT" => {
                            let mut body = String::new();
                            request.as_reader().read_to_string(&mut body).unwrap();
                            let mut files = f.lock().unwrap();
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                files.entry(path)
                            {
                                entry.insert(body);
                                if fail.swap(false, Ordering::SeqCst) {
                                    (500, String::new())
                                } else {
                                    (201, String::new())
                                }
                            } else {
                                (412, String::new())
                            }
                        }
                        "GET" => f
                            .lock()
                            .unwrap()
                            .get(&path)
                            .map(|b| (200, b.clone()))
                            .unwrap_or((404, String::new())),
                        "DELETE" => {
                            f.lock().unwrap().remove(&path);
                            (204, String::new())
                        }
                        _ => (405, String::new()),
                    };
                    let _ = request.respond(
                        Reply::from_string(body).with_status_code(code).with_header(
                            Header::from_bytes("Content-Type", "application/xml").unwrap(),
                        ),
                    );
                }
            });
            Self {
                base,
                files,
                stop,
                fail_put,
                worker: Some(worker),
            }
        }
        fn client(&self, store: SharedStore) -> Dav {
            Dav::with_base(self.base.clone(), "test", "password".into(), store).unwrap()
        }
    }
    impl Drop for MockDav {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.worker.take().unwrap().join().unwrap();
        }
    }
    fn db() -> SharedStore {
        Arc::new(Mutex::new(Store::open(":memory:").unwrap()))
    }
    fn content(s: &str) -> Content {
        Content {
            text: s.into(),
            ..Content::default()
        }
    }

    #[tokio::test]
    async fn real_http_two_devices_retry_conflict_and_delete() {
        let server = MockDav::start();
        let (a, b) = (db(), db());
        let (da, dbb) = (server.client(a.clone()), server.client(b.clone()));
        da.test_connection().await.unwrap();
        assert!(server.files.lock().unwrap().is_empty());
        let n = a
            .lock()
            .unwrap()
            .save(None, None, content("start"))
            .unwrap();
        server.fail_put.store(true, Ordering::SeqCst);
        assert!(da.sync().await.is_err()); // Server saved PUT but response failed.
        da.sync().await.unwrap();
        dbb.sync().await.unwrap();
        assert_eq!(b.lock().unwrap().note(&n.id).unwrap().content.text, "start");
        a.lock()
            .unwrap()
            .save(Some(&n.id), Some(&n.head_id), content("A offline"))
            .unwrap();
        b.lock()
            .unwrap()
            .save(Some(&n.id), Some(&n.head_id), content("B offline"))
            .unwrap();
        da.sync().await.unwrap();
        dbb.sync().await.unwrap();
        da.sync().await.unwrap();
        let n = a.lock().unwrap().note(&n.id).unwrap();
        assert_eq!(n.conflicts.len(), 1);
        let mut heads = vec![n.head_id.clone()];
        heads.extend(n.conflicts.iter().map(|r| r.id.clone()));
        a.lock()
            .unwrap()
            .resolve(
                &n.id,
                heads,
                Content {
                    deleted: true,
                    ..content("merged")
                },
            )
            .unwrap();
        da.sync().await.unwrap();
        dbb.sync().await.unwrap();
        let b_note = b.lock().unwrap().note(&n.id).unwrap();
        assert!(b_note.content.deleted);
        assert!(b_note.conflicts.is_empty());
    }

    #[test]
    fn listing_rejects_outside_paths_and_foreign_origins() {
        let root = Url::parse("https://example.com/QingNote/v1/").unwrap();
        let uid = id();
        let xml=format!("<multistatus xmlns='DAV:'><response><href>/QingNote/v1/{uid}.json</href></response><response><href>https://evil.test/{uid}.json</href></response><response><href>/other/{}.json</href></response></multistatus>",id());
        assert_eq!(parse_listing(&xml, &root).unwrap(), HashSet::from([uid]));
    }
}
