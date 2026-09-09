# 上传 GitHub 与发布安装包

源码进入 Git 仓库，安装包放在 Release 附件。不要提交构建缓存、个人便签、备份或凭据；已有忽略规则不能替代上传前检查。

## 首次上传

在 GitHub 新建空仓库，自行选择公开或私有；不要预先生成 README、License 或 .gitignore。

本地源码已准备为 main 分支提交。在项目目录运行，替换实际仓库地址：

```sh
git remote add origin git@github.com:haohaomiao/haonote.git
git push -u origin main
```

通过本机 Git 凭据管理器或 SSH 认证，不要将密码或令牌写进地址或发送到聊天。已有 origin 时，先用 `git remote -v` 确认目标，不要盲目覆盖。

## 自动构建与发布

推送 main 后，Actions 检查并构建 Linux / Windows 安装包。确认代码后创建版本标签：

```sh
git tag -a v0.1.4 -m "haonote 0.1.4"
git push origin v0.1.4
```

两个平台检查通过后，工作流先创建草稿并上传安装包、SHA256SUMS 和版本说明，附件上传完成后自动公开为预发布版。推送版本标签即表示要公开发布，测试时只推分支，不要推版本标签。仓库需允许 Actions 运行及发布作业请求的 contents: write 权限。已发布标签不覆盖，修复后使用新版本号。

也可手动创建 Release，选择对应源码标签，粘贴 `docs/releases/v0.1.1.md`，上传 `artifacts/releases/0.1.1/` 中三个安装包及 SHA256SUMS。不要同时手动创建同一版本和触发自动草稿，以免冲突。

后续版本需同步更新 package.json、package-lock.json、src-tauri/Cargo.toml、Cargo.lock、src-tauri/tauri.conf.json 和对应版本说明，再创建新标签。工作流使用 GitHub 作业临时令牌，无需额外服务器或长期令牌。

## 本地交付

安装包、校验清单和源码 ZIP 位于 `artifacts/releases/0.1.1/`。源码 ZIP 只包含 Git 跟踪的文件，不含构建缓存、运行资料库或 `.git`。上传源码不会自动上传这里的安装包。

日常调试使用 `npm ci`、`npm run desktop`，无需反复安装。
