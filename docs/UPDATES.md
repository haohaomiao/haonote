# 应用内更新

此功能已搁置，尚未发布。当前发布流程只生成普通安装包和 SHA256SUMS，不需要签名密钥，不生成 latest.json 或更新渠道。应用内公钥保持为空，更新不启用。

下文保留的是将来恢复自动更新时的设计与配置参考，不代表当前发布流程。恢复时还需要重新接入签名构建、验证和更新清单生成。已发布的 0.1.4 没有更新代码，必须手动安装一次支持更新的新版。

## 使用方式

Explorer 左下角提供「检查更新」。正式安装版默认在启动约 15 秒后检查一次，运行期间每 6 小时检查一次；可在「更新设置」关闭。检查只获取公开版本信息，不上传便签或同步凭据。

发现版本 → 下载更新（仍可编辑）→ 验证签名 → 用户点击「重启更新」→ 暂停编辑并等待所有便签保存 → 安装并重新打开。

下载或签名失败不安装。保存失败、输入法仍在组词或窗口 8 秒未回应时取消安装，恢复编辑。新建、关窗与退出在安装准备阶段被阻止。更新沿用应用标识和 SQLite 数据库；未关闭的便签通过现有窗口恢复机制重开。更新不要求云同步成功，本地尚未上传的修改保留。

Windows NSIS 安装器使用 passive 模式，并要求安装完成后重开应用。Linux 仅 AppImage 应用内更新；deb 用户安装新的 deb 包。开发版不安装正式版更新。安装器失败、权限限制、断电和系统拦截仍需人工处理；建议重要数据定期导出备份。

## 一次性签名配置（维护者）

1. 在仓库外生成更新密钥。示例命令不覆盖已有密钥，不要添加 `--force`：

   ```sh
   mkdir -p -m 700 /home/hhm/code/.haonote-signing
   umask 077
   npm run tauri -- signer generate --ci --password '' --write-keys /home/hhm/code/.haonote-signing/haonote.key
   ```

   这是无口令私钥，依靠文件权限和 GitHub Secrets 保护。也可使用加密私钥，并额外配置密码 Secret。私钥不可提交 Git、发到聊天或作为构建附件。请另行安全备份，丢失后旧客户端无法验证新密钥签署的更新。`.key.pub` 是可公开的公钥。

2. 将 `.key.pub` 文件中的公钥字符串填到 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。只提交公钥，不能填私钥。

3. 打开仓库 Settings → Secrets and variables → Actions → New repository secret：

   - `TAURI_SIGNING_PRIVATE_KEY`：私钥文件内容，不是本机文件路径。
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥口令；无口令时可不设置。

4. 更新各处版本号并增加版本说明，再发布新的版本标签。标签构建必须有签名密钥；普通 main / PR 构建不使用签名密钥。

## 发布流程

标签构建生成 Windows `.exe.sig` 和 Linux `.AppImage.sig`，然后运行 `verify_updates`，检查安装包签名是否与应用内公钥匹配。`scripts/release-assets.mjs` 检查必需文件、生成带签名及版本专属下载地址的 `latest.json` 和 SHA256SUMS。客户端官方插件再次验证签名，不以 SHA256 文件代替签名。

只有两个平台构建及测试全部成功，才公开数字版本 Release；然后把该版本的清单上传到固定的 `update-channel` 预发布 Release。客户端地址为：

`https://github.com/haohaomiao/haonote/releases/download/update-channel/latest.json`

使用单独渠道是因为 GitHub 的 `/releases/latest` 不包含预发布版。本渠道目前跟随 haonote 的公开预发布版本；不自动降级。发布任务串行更新渠道，旧版本构建不允许回退清单版本。

GitHub 下载在不同网络下可能失败，程序会给出可重试提示；没有绕过 TLS 校验或签名校验的选项，不需要坚果云应用密码。若以后部署镜像，必须保持 HTTPS 和同一签名公钥。

## 验收边界

自动化覆盖更新 UI 的模拟成功／失败顺序、保存屏障，以及清单生成。正式宣布可用前，还应在 Windows 和 AppImage 测试机上完成一次真实的“旧签名版本 → 新签名版本”升级，验证未保存输入、窗口恢复、安装器权限和失败恢复。

当前本地验证通过：17 项界面测试、17 项核心测试、3 项发布清单测试、Linux 原生窗口回归，以及前端构建和 Rust Clippy。真实跨版本签名安装尚未验收，不等同于已发布可用。
