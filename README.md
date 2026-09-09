# haonote

原名 QingNote（轻笺），现更名为 haonote。为兼容已有便签，保留旧版应用标识、数据库文件名、凭据库键、备份格式和默认 WebDAV 目录 `QingNote`，不需要迁移或重建同步目录。内部 Rust 包名及可执行文件名暂保留 `qingnote`，界面和安装包使用新名称。

一个以 Windows、Linux 为目标的简洁桌面便签。打开就写，本地自动保存，通过自己的坚果云或 WebDAV 账号同步。

当前为 **0.1.4 预发布版**。安装包见 [GitHub Releases](https://github.com/haohaomiao/haonote/releases)，更新内容见 [0.1.4 版本说明](docs/releases/v0.1.4.md)。Windows 安装包尚未实机验证；手机安装包尚未实现。

## 使用

- 每张便签是独立窗口，可置顶、缩放、换色、调整字号。
- 双击标题栏文字区域，只留下标题栏；再次双击恢复原来的大小。不再单独放置折叠按钮，右键菜单仍保留折叠操作。
- 右键单击标题栏打开原生菜单：折叠、置顶、Markdown、颜色、透明度、大小、新建、列表、归档与回收站。折叠状态下也能正常弹出。
- 「便签选项」点击外部、窗口失去焦点或按 Esc 会关闭。
- 右键「编辑标题」或按 F2，可编辑独立标题；也可以在「便签选项」中填写。留空时仍以正文第一行作为显示标题。标题随正文同步，支持搜索、备份、冲突合并；同步设备都需要更新至包含独立标题功能的版本，旧版本会拒绝包含新字段的便签。
- 新便签默认使用可直接输入的「排版编辑」，没有常驻格式工具栏。选中文字后右键选择加粗、斜体、下划线，立即看到效果；也支持 `Ctrl/Cmd+B`、`Ctrl/Cmd+I`、`Ctrl/Cmd+U` 和撤销/重做。此前保存的源码模式偏好会保留。
- 正文右键菜单提供剪切、复制、粘贴；支持系统 `Ctrl/Cmd+X/C/V` 快捷键，富文本粘贴保留支持的格式并过滤危险 HTML。排版编辑中拖动选区默认移动，单次撤销可恢复原位置；从外部拖入内容不会删除外部来源。
- 字体和字号收在正文右键菜单、标题栏「便签选项」中；作用于整张便签，按本机便签保存。实际字体取决于系统已安装字体。
- 拖动右下角调整大小；「便签选项」可输入宽高，并调整 30%–100% 不透明度（越低越透明）。Linux 桌面需要启用窗口合成才能透出桌面。
- 点击底部「源码 / 排版」切换 Markdown 源码和直接排版编辑，支持标题、列表、可勾选任务、引用、代码块和表格。正文仍以 Markdown 保存；下划线沿用 `<u>文字</u>`，没有新存储格式或后端服务。
- 大小、折叠、透明度、预览模式按便签保存在本机，重启后恢复，不影响其他设备的布局。Markdown 源码仍作为正文同步。
- 收起窗口不删除内容；从列表可以再次打开。
- 归档、回收站、搜索、JSON 完整备份和 TXT 导出。
- 列表窗口支持卡片／紧凑列表切换；按创建或修改时间升序／降序排序，支持今天、近 7 天、近 30 天筛选（包含今天，以本机日历日期为界，跟随所选排序时间字段）。搜索同时匹配标题和正文，不区分大小写，空格分隔的关键词需要全部匹配；可与分类及时间筛选组合。Ctrl/Cmd+F 定位搜索框，一键清除搜索和筛选。浏览方式、排序和时间范围保存在本机，不参与同步；搜索词不持久保存。
- 不连接云服务也能正常使用。
- 正常退出使用列表左下角或托盘的「退出haonote」。程序会等待各便签完成本地保存。
- `Ctrl/Cmd + N` 新建、`Ctrl/Cmd + F` 搜索（列表窗口）；`Ctrl/Cmd + S` 立即保存、`Esc` 收起（便签窗口）。

没有实际编辑时，切换模式不重写原 Markdown。实际排版编辑后可能规范化标记和空白。包含图片、原始 HTML、脚注或无法可靠往返转换的内容，会保留源码、显示只读预览并提示切换源码修改，不会静默删除内容。仅无属性的 `<u>` 是允许的 HTML 扩展；不加载远程图片，也不会在便签里导航链接。粘贴 HTML 会先过滤危险标签与资源加载属性。

排版编辑使用 [Tiptap Markdown](https://tiptap.dev/docs/editor/markdown/getting-started/basic-usage)，只读预览经过 [DOMPurify](https://github.com/cure53/DOMPurify) 过滤。所有依赖本地打包，没有新增联网编辑服务。

升级沿用原正式版资料库与同步配置，安装前请导出 JSON 备份并正常退出旧版。安装说明见 [版本说明](docs/releases/v0.1.4.md)，上传源码见 [GitHub 发布指南](docs/GITHUB.md)。

### 配置坚果云

1. 在坚果云网页版的「账户信息 → 安全选项 → 第三方应用管理」添加“haonote”，生成应用密码。
2. 打开haonote「设置与同步」，填写：

   | 项目           | 内容                              |
   | -------------- | --------------------------------- |
   | 服务地址       | `https://dav.jianguoyun.com/dav/` |
   | 账号           | 坚果云注册邮箱                    |
   | 第三方应用密码 | 刚生成的应用密码，不是登录密码    |
   | 同步目录       | `QingNote`，所有设备使用同一目录  |

3. 点击「测试并保存连接」。测试会创建目录，并写入、读取和删除一个独立临时文件。
4. 在另一台设备填写相同账号和目录，即可同步。无需把凭据发给开发者，也无需购买服务器。

应用密码保存在系统凭据库；凭据库不可用时只保留在进程内存，重启后重新填写。账号和地址存储在本机配置中。同步仅支持 HTTPS，不跟随重定向，不绕过证书验证。

第一版绑定后不能直接切换账号/目录，防止把一个账号的便签无意上传到另一个账号。支持暂停同步、更新应用密码。其他 WebDAV 服务可在首次连接时填写其地址。

### 同步行为

- 约每 2 分钟检查一次；启动、列表恢复焦点和手动同步也会触发检查，最短间隔 20 秒。
- 本机输入约 250 毫秒后保存到 SQLite，中文输入法组词期间不打断输入。
- 连续修改会合并到本地草稿；准备上传时封存为不可变版本。多台设备的并发修改显示为冲突，用户选择版本或合并。
- 回收站不自动清空；云端删除标记和历史版本也不自动移除，避免长期离线设备把旧内容复活。
- 每台设备每 30 分钟最多发起 150 个 WebDAV 请求，超额后自动等待。初次同步大量历史数据可能需要多轮；不承诺秒级同步。
- 仅传输新增版本，不上传整个 SQLite 文件。其他使用同一坚果云账号的应用也会消耗服务方额度。
- 第一版不清理远端版本历史，长期高频使用会增加目录列表和初次同步的成本。

### 从 Simple Sticky Notes 迁入

列表底部「导入旧便签」可选择 Simple Sticky Notes 的 `.db` 备份。请使用来源软件「设置 → Database → Backup」生成的文件，或先完全退出来源软件再复制 `Notes.db`；不要复制仍在写入的数据库。

导入只读源文件，合并而不覆盖现有便签。支持标题、纯文本正文、近似颜色、创建/修改时间；来源回收站内容仍进入回收站。RTF 格式、图片、闹钟、笔记本分组与窗口布局不迁入，原始备份应保留。来源日期没有时区，按原墙上时间标记为 UTC，显示时可能存在时区偏差。正文仍按 haonote 的 Markdown 规则显示，需要原样查看时切换源码。

相同来源 ID 和创建时间只导入一次，即使之后修改来源或在 haonote 中编辑、删除，重复导入也不会覆盖或复活便签。这是一次性迁移，不是两个应用之间的双向同步。导入后按 haonote 当前云同步设置同步；导入前会确认。每个备份最大 32 MB、10000 条，单条正文最大 128 KB；发现不支持的结构或无效便签时整批取消，不部分导入。

### 本地资料库与备份

便签数据库位于设置页显示的「本地资料库」目录。Windows 使用应用数据目录，Linux 通常为 `~/.local/share/app.qingnote.desktop/`。不要让网盘直接同步正在使用的 SQLite 文件。

无广告、遥测和开发者账号服务器。正文以 JSON 保存在你的 WebDAV 服务；HTTPS 保护传输，但**当前没有端到端加密**，服务商和有账号访问权限的人可以读取正文。本地 SQLite 同样未加密。

建议定期点击列表底部「导出」保存 JSON 备份。导入会合并版本，保留当前数据；重复导入不会重复生成便签。TXT 是阅读格式，不能用于完整恢复。

## 开发与构建

需要 Node.js 20.19+（建议 22）、Rust stable，以及 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。

Ubuntu 24.04：

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libayatana-appindicator3-dev librsvg2-dev patchelf libssl-dev
npm ci
npm run desktop
```

### 不安装的调试方式

```sh
cd qingnote  # 换成实际项目路径
npm run desktop
```

这会打开「haonote · 开发版」，直接运行真实桌面窗口，**不需要打包或安装**。修改 Svelte / CSS / TypeScript 会热更新；修改 Rust 会自动重编译并重启。首次启动需编译，后续为增量构建。在启动终端按 `Ctrl+C` 停止。界面热更新可能重建编辑器，调试时使用测试便签即可。

开发版标识为 `app.qingnote.development`，与正式版 `app.qingnote.desktop` 的 SQLite、显示偏好和同步配置分开。默认不连接云服务；若需测试同步，建议使用独立 WebDAV 测试目录。开发启动脚本优先使用系统 Rust；在当前工作区也能自动找到此前安装的隔离工具链，无需手动设置 PATH。

只调界面时用 `npm run dev`，打开 `http://127.0.0.1:1420`。浏览器预览无法验证真实置顶、原生菜单、窗口尺寸或透出桌面的效果；这些请用 `npm run desktop`。浏览器右键标题栏会打开界面内选项作为替代。

Windows 需要 Visual Studio C++ Build Tools、Rust MSVC 工具链、WebView2。安装依赖后同样运行 `npm ci` 和 `npm run desktop`。

```sh
npm run build                         # 类型检查和前端生产构建
cargo test -p qingnote-core --locked # SQLite 与模拟 WebDAV 测试
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm run test:ui                      # 先执行 npx playwright install chromium

npm run tauri -- build --bundles deb,appimage # Linux
npm run tauri -- build --bundles nsis         # Windows
```

安装包输出到 `target/release/bundle/`。GitHub Actions 在推送 main 时检查并构建 Windows、Linux 安装包；推送匹配版本的 `v*` 标签时，会在两个平台检查通过、附件上传完成后公开发布预发布版本。不要把测试标签推送到远端。

浏览器预览：`npm run dev`，访问 `http://127.0.0.1:1420`。预览只用于界面调试，使用浏览器本地数据，不连接 WebDAV，也不包含桌面能力；生产构建会去掉预览适配器。

### 原生桌面测试

Linux 安装 `xvfb`、`openbox`、`x11-utils`、`xdotool`、`xcompmgr`、`webkit2gtk-driver` 和 `cargo install tauri-driver --locked` 后：

```sh
npm run build
cargo build -p qingnote --features custom-protocol
XCOMPMGR_PATH=/usr/bin/xcompmgr xvfb-run -a dbus-run-session -- node tests/native.mjs
```

该测试创建临时资料库，检查真实 Tauri 窗口、SQLite 保存和进程重启。不会访问你的便签或坚果云。可通过 `TAURI_DRIVER`、`WEBKIT_DRIVER`、`QINGNOTE_BINARY` 指定测试程序路径。

## 代码结构

```text
src/                    Svelte 界面、轻量 IPC 适配
crates/core/src/
  model.rs              便签、版本和配置
  store.rs              SQLite、草稿合并、冲突与备份
  sync.rs               WebDAV 请求和增量版本同步
src-tauri/src/
  lib.rs                启动和定时同步
  commands.rs           界面命令、凭据库、导入导出
  windows.rs            独立窗口、托盘、窗口位置
tests/                  界面与原生桌面验证
```

没有额外服务器、消息队列、ORM、插件架构或业务状态管理框架。同步协议细节见 [ARCHITECTURE.md](docs/ARCHITECTURE.md)。
