# MyVideo

MyVideo 是从原 HGhub 项目拆分出来的独立 Android 视频客户端。

**MyVideo 不连接黄果 API，也不包含黄果账户、登录、短剧推荐或黄果搜索入口。**
它只使用用户选择的视频来源的公开网页和播放线路。原 HGhub 版本仍保留在 `main` 分支。

## 两个独立版本

| 版本 | Git 分支 | Android 应用标识 | 显示名称 |
| --- | --- | --- | --- |
| HGhub | `main` | `com.hghub.huangguo` | HGhub |
| MyVideo | `myvideo` | `com.daneg.myvideo` | MyVideo |

因为应用标识不同，两个 APK 可以同时安装，且不共用应用数据和离线文件。

## MyVideo 提供的功能

- 多站公开网页搜索，已针对努努影院编写搜索、选集和 HLS 播放适配器。
- 在 App 内播放普通 HLS / MP4 / WebM 视频，取决于来源兼容性。
- 将无 DRM、允许保存的 MP4/WebM 或完整、普通 TS HLS 下载到 App 私有目录。
- 离线播放已完成下载的视频；努努影院支持手动标记并跳过约 6 秒的视频内广告区间。
- 视频网站需要单独适配；输入新网址不会自动保证播放和下载能力。

## 构建 Android ARM64 APK

在 GitHub 仓库的 **Actions → Build MyVideo Android APK → Run workflow** 点击运行。
该 workflow 固定检出 `myvideo` 分支，构建并上传 `MyVideo-Android-ARM64-APK` 产物，
APK 文件名为 `MyVideo_1.0.0_android_arm64.apk`。

构建任务执行完成以后才能下载 APK。该 APK 使用 Android debug 测试签名，
不应视作已完成实机验收的发行版。

## 网络与安全

运行时 Rust 入口只注册公开视频网页请求、HLS 代理、在线播放和离线下载命令；
没有黄果 API 域名、账户会话或业务请求信封。
公网 HTTPS 请求保留内网目标阻断及逐跳重定向校验。
项目遵循原仓库的 PolyForm Noncommercial 许可证；影片保存还应遵守适用的版权和网站授权约束。
