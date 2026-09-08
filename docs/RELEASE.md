# 0.1.1 发布候选记录

已生成 Windows / Linux 安装包，更新内容见 [版本说明](releases/v0.1.1.md)。本版为预发布版本；Windows 实机和真实云同步仍需验收。

## 安装与升级

本地交付目录为 `artifacts/releases/0.1.1/`：

- Ubuntu 24.04 x86_64：`QingNote_0.1.1_amd64.deb`
- Linux x86_64：`QingNote_0.1.1_amd64.AppImage`
- Windows x64：`QingNote_0.1.1_x64-setup.exe`

先导出 JSON 备份，正常退出旧版，再安装新版。正式版沿用原资料库；开发版资料库独立，需要手动导出、导入。同步设备请全部升级，旧版本会拒绝新增的独立标题字段。

在项目根目录运行：

```sh
sha256sum --check docs/SHA256SUMS
sudo apt install ./artifacts/releases/0.1.1/QingNote_0.1.1_amd64.deb
```

AppImage 需要可执行权限和相应运行环境；基于 Ubuntu 24.04 构建，不承诺旧发行版兼容。Windows 安装包未签名；缺少 WebView2 时需联网下载微软运行时，离线机器请预先安装。

## 验证范围

- 类型检查、前端生产构建、Rust Clippy 严格检查及格式检查通过。
- Rust 核心测试 15 项、浏览器测试 7 项通过，覆盖持久化、同步冲突、备份、直接排版编辑、选区格式、源码保留与安全粘贴。
- 最终 Debian 安装包解出的程序通过 Linux 原生回归，覆盖真实窗口折叠、原生菜单格式、尺寸、透明度、标题、字体、保存与重启恢复。测试脚本补充等待异步菜单展开，避免过早查询控件；未在系统中执行安装。
- Windows 已交叉编译并用 NSIS 打包，尚未实机运行。GitHub 工作流提供 Windows 原生构建。

前端单文件约 690 KB（gzip 223 KB），有构建体积提示，不影响构建。Markdown 编辑依赖本地打包的 Tiptap / ProseMirror，无额外后端服务。

仍需人工检查 Windows 安装与中文输入法、真实坚果云双设备同步，以及实际 Linux / Wayland 桌面集成。未实现移动端、端到端加密、远端历史压缩和自动更新。

上传步骤见 [GitHub 发布指南](GITHUB.md)。本地与 CI 构建的校验值可能不同，应使用对应附件的校验清单。此前开发记录归档于 [历史记录](releases/development-history.md)。
