# 新平台接入规则

本文档定义新平台接入和旧平台迁移的强制规则。涉及平台独立安装、卸载、更新、远端热更新、平台包 UI 或 sidecar adapter 的内容，必须同时遵守 [`docs/platform-hot-update-architecture.md`](./platform-hot-update-architecture.md)。

## 1. 平台类型

每个平台必须明确声明平台类型：

1. `bundled`：平台能力随主应用发版，不展示独立安装、卸载、更新、包大小、更新日志等平台包交互。
2. `hotUpdate`：平台能力由独立平台包提供，可以在不更新主应用的情况下独立安装、卸载、更新。

`hotUpdate` 平台必须继续声明安装边界：

1. `coreNativeBoundary`：只把平台包生命周期、入口状态和运行 gate 独立出来，业务 UI 或业务 native command 仍在主应用内，不能宣称业务代码可远端热更新。
2. `sidecarAdapter`：平台业务命令通过包内 sidecar adapter 的稳定协议执行，业务逻辑可以随平台包发版。

`coreNativeBoundary` 只允许作为分阶段迁移状态使用；manifest/runtime 必须列出仍在宿主里的 `nativeBoundaries`，并在文档和更新日志中明确这些业务命令尚未完成远端热更新。当前项目的最终目标是“平台相关的一切都支持独立热更新”，因此 `coreNativeBoundary` 不能作为完成态；最终验收必须把平台升级为 `sidecarAdapter` 并清空 `nativeBoundaries`。

## 2. 推荐架构

新平台或高保真迁移平台的默认架构是：

```text
Core Shell + Platform Package + Remote React UI + Sidecar Adapter + runtimeReady gate
```

职责边界：

1. Core Shell 只保留平台包生命周期、状态、入口容器、平台页外壳、右上角平台包操作区、通用不可用页、Host API、Tauri command facade 和 adapter runner。
2. Platform Package 承载 `manifest.json`、`runtime/index.json`、`ui/remoteEntry.js`、`ui/style.css`、sidecar adapter、changelog、包大小和平台资源。
3. 已迁移平台的业务 UI、tabs、筛选、账号卡片、表格、空态、弹框、实例页和 runtime 业务区必须由平台包 remote UI 提供。
4. 已迁移平台的账号、OAuth、切号、配额、runtime 等业务能力必须通过 sidecar adapter 提供。
5. 后台刷新、token keeper、Web report、浮动卡片、托盘/菜单业务数据、账号迁移、导入导出、数据备份/恢复、实例和本地网关等平台相关能力也属于平台包边界，最终必须通过 remote UI、Host API 或 sidecar adapter 间接执行，禁止长期留在宿主 Rust/Tauri 里。

### 2.1 Host Event Bridge

批量导入、批量测试、流式聊天、OAuth 回调、任务调度和确认类业务如果需要向前端持续推送事件，必须使用通用 Host Event Bridge：

1. Core Shell 启动平台 adapter 时注入 `COCKPIT_HOST_EVENT_URL` 和 `COCKPIT_HOST_EVENT_TOKEN`。
2. adapter 执行业务时向该 URL `POST` `{"event":"事件名","payload":{...}}`。
3. Core Shell 只负责 token 校验和 `app.emit(event, payload)` 转发，不得计算、改写或补充平台业务状态。
4. 事件名和 payload 必须保持迁移前前端监听格式一致；remote UI 不应因为迁移 sidecar 而改事件协议。
5. 没有 Host Event Bridge、轮询状态协议或持久化 session 的长任务，必须继续列入过渡 `nativeBoundaries`，禁止只删除 boundary。

### 2.2 平台日志边界

`sidecarAdapter` 平台必须按插件式边界写日志，禁止把平台业务日志长期混在 Core Shell 的 `app-YYYY-MM-DD.log` 里，也禁止直接丢弃 adapter stderr：

1. Core Shell 只写主应用生命周期、平台包生命周期、adapter 启停、Host Event Bridge 和通用错误到 `app-YYYY-MM-DD.log`。
2. Codex API 请求诊断等跨账号请求日志继续写入专用 `codex-api-YYYY-MM-DD.log` 或结构化请求日志库，不应混入普通平台运行日志。
3. 每个 sidecar adapter 必须在启动时初始化共享 logger，并按 `platform-<platformId>-YYYY-MM-DD.log` 写入自己的业务日志，例如 `platform-codex-2026-06-24.log`。
4. Core Shell 启动 adapter 时必须注入 `COCKPIT_PLATFORM_LOG_FILE_PREFIX=platform-<platformId>`，adapter 只能把普通 tracing 日志写入该文件，不能向 stdout 输出普通日志；迁移期可兼容旧 `platform-<platformId>.log` env 值，但新实现必须使用无 `.log` 的 basename。
5. 宿主 command facade 若输出平台业务日志，例如 `[Zed Command]`、`[Kiro Command]`、`[Codex ...]`，必须由 logger 路由到对应平台日志，不能继续混入 Core Shell 主日志。
6. adapter stdout 第一行只允许输出 `http-json-v1` 启动握手 JSON；后续 stdout/stderr 仅作为异常诊断被 Core Shell 捕获，不能作为主要业务日志通道。
7. 日志查看器必须把 `app-*.log`、`codex-api-*.log` 和 `platform-*.log` 都视为受管理日志。Core Shell 启动时必须自动迁移旧 `app.log.YYYY-MM-DD`、`codex-api.log.YYYY-MM-DD`、`platform-<platformId>.log.YYYY-MM-DD`、`app.log`、`codex-api.log`、`platform-<platformId>.log` 物理文件名；目标文件不存在时直接重命名，目标文件已存在时先合并旧内容再删除旧文件。下拉框保留旧文件显示别名只作为迁移失败或外部遗留文件的兼容兜底。
8. 新增平台包或迁移平台时，平台文档、contract 校验或手动验收必须覆盖“触发一次 adapter 调用后日志目录出现对应 `platform-<platformId>-YYYY-MM-DD.log`”。

### 2.3 隐藏后台入口与 DTO 边界

迁移平台时必须同时处理页面外的隐藏入口，不能只改账号页：

1. `provider_token_keeper`、`web_report`、`provider_current`、托盘、macOS 原生菜单、浮动卡片、自动刷新、账号迁移、数据备份/恢复、路径重试和启动恢复都必须先过 `runtimeReady` gate。
2. 这些入口不得直接调用平台业务模块或解释平台账号刷新规则；必须调用平台 adapter 暴露的窄方法，例如 `accounts.keepaliveDue`、`quota.refreshAll`、`accounts.list`、`accounts.pickAutoSwitchTarget`、`instances.store.get`、`instances.store.replace`、`runtime.detectLaunchPath`。
3. 宿主 command facade 需要平台返回结构时，类型必须放在共享 `models` 层，或使用 `serde_json::Value` 透传；禁止为了 DTO 复用重新 import `crate::modules::<platform>`。
4. 已迁移平台的旧业务模块不得继续出现在 `src-tauri/src/modules/mod.rs`；旧源码可以留在 adapter/core crate 侧复用，但不能编译进 Core Shell。
5. 完成迁移后必须执行 `npm run audit:platform-full-hot-update`，确认 manifest/runtime/index、模块声明和直接引用都没有平台业务残留。

## 3. UI runtime

需要页面 UI、tabs、弹框、筛选、账号卡片、交互随平台包热更新时，必须使用：

```json
{
  "protocol": "react-remote-esm-v1",
  "entry": "ui/remoteEntry.js",
  "style": "ui/style.css",
  "exports": ["mount", "unmount"]
}
```

强制规则：

1. remote module 必须导出 `mount(container, hostApi)`。
2. remote module 应导出 `unmount(container)` 或由 `mount` 返回 cleanup。
3. 宿主只能从本地已安装且校验通过的平台包目录加载 remote JS/CSS。
4. 禁止直接执行 GitHub raw 上的 JS。
5. 构建产物必须真实保留 `mount/unmount` ESM 导出，构建脚本必须在打包前验证产物导出，禁止只写 manifest 声明但产物不可 import。
6. 构建产物必须能在 WebView 浏览器环境运行，禁止残留 `process.env`、Node-only global 或其它未通过 Host API 提供的运行时依赖。
7. remote CSS 必须 scoped 到平台 root，禁止覆盖 `html`、`body`、`#root`、`:root`、全局 `*` 或宿主布局背景；宿主已经加载的基础设计系统样式不得在 remote 包里重复注入并覆盖。
8. 平台业务 tabs 必须由 remote UI 渲染；宿主如需保持原页面视觉位置，只能提供空的 remote tabs slot，并通过 `hostApi.tabsSlotId` 暴露给 remote UI。
9. `iframe-html-v1` 只允许作为第三方强隔离插件的可选方案，不作为本项目平台原样迁移主路径。
10. 安装且 `runtimeReady=true` 后，平台业务区必须尽量保持迁移前页面布局、样式、交互、空态、弹框和操作流，不得为了热更新重写一套明显不同的 UI。

## 4. 页面与入口交互

入口显示和业务可用必须分离：

1. 侧边栏、仪表盘、托盘/菜单栏是否显示，只取决于用户布局配置、平台 contribution 和远端隐藏配置。
2. 热更新平台即使未安装，只要用户勾选显示，也应出现在侧边栏和仪表盘，并展示未安装、可更新、需修复或不兼容等短状态。
3. 侧边栏和仪表盘只展示状态并导航到平台页，不执行安装、更新、卸载或修复。
4. 平台布局弹框只管理入口显示和排序，不执行安装、更新、卸载或修复。
5. 平台页必须始终可打开。
6. 平台页右上角使用不参与页面布局的紧凑平台包操作区，已安装态默认提供“检查更新 + 卸载 + 更多”三个入口；可更新态提供“更新 + 卸载 + 更多”；未安装或需修复态提供“下载/修复 + 更多”。入口不得显示平台名、平台图标或版本号，避免与业务页标题重复。
7. “更多”菜单只承载常态管理命令，例如检查更新、更新日志、安装、修复、更新、卸载；禁止在菜单里嵌入大块更新日志预览。点击“更新”必须打开与主应用更新体验一致的平台更新弹框，弹框中展示目标版本、包大小、多语言更新日志，并提供跳过/取消与更新操作。
8. 平台更新日志属于平台包元数据，应放在 manifest/runtime/远端 index 可同步的数据里。`changelog[].notes` 是默认文案；如需多语言，必须在 `changelog[].locales[locale].notes` 提供对应语言，Core Shell 按当前语言、同语系、英文、默认 notes 的顺序回退。禁止把平台包更新日志写死到主应用 locale。
9. 未安装或 `runtimeReady=false` 时，平台页显示通用不可用页；不可用页可以提供安装或修复主按钮，但必须复用平台包生命周期逻辑和二次确认弹框。
10. 未安装或 `runtimeReady=false` 时，不加载 remote UI，不读取账号，不启动 OAuth，不切号，不后台刷新配额。
11. 账号迁移、数据备份/恢复、导入导出、设置页账号覆盖、浮动卡片、Web report、provider current、token keeper、路径重试、托盘、macOS 原生菜单、自动刷新和 App 路由等全局工具也必须 respect `runtimeReady`；未安装时只能跳过或提示平台不可用，禁止调用平台业务命令。
12. 安装、修复、更新、卸载必须二次确认，失败必须显示在当前弹框或当前操作区。

## 5. Artifact 与远端更新

远端索引只负责发现版本和下载地址：

1. 每个平台包必须按 `os + arch` 提供独立 artifact。
2. Core Shell 只能下载当前系统匹配的 artifact。
3. artifact 必须包含真实 `downloadSizeBytes` 和 `sha256`。
4. 只包含某一系统 adapter 的包，不得声明为其它系统可用。
5. GitHub Actions 应分别构建 macOS、Windows、Linux adapter 和 zip。
6. 平台包 zip 必须通过 `npm run package:platform` 生成，禁止手写临时压缩命令。
7. 多系统远端 artifact 必须使用 `os-arch` 文件名，例如 `zed-0.26.7-macos-aarch64.zip`；本地兼容包才允许继续使用 `zed-0.26.7.zip`。
8. `.github/workflows/platform-packages.yml` 是标准跨系统构建入口；CI 每个 runner 只输出当前 OS/arch 的 zip 和 metadata JSON，不直接改写远端 index。
9. 远端 `platform-packages/index.json` 必须通过 `npm run package:platform-index` 基于各 OS/arch metadata 汇总生成，确认 size、sha256、downloadUrl 和 artifact 覆盖后再发布。

### 5.1 远端测试通道

任何平台包远端测试都必须先进入独立 test channel，不得直接把测试包写入正式通道：

1. 测试桌面端必须使用 `src-tauri/tauri.test.conf.json`，bundle id 为 `com.jlcodes.cockpit-tools.test`，数据目录为 `~/.antigravity_cockpit_test`。
2. 测试桌面端 updater endpoint 固定为 `https://github.com/jlcodes99/cockpit-tools/releases/download/test-latest/latest-test.json`；正式用户不会读取该地址。
3. 测试平台包索引固定为 `https://raw.githubusercontent.com/jlcodes99/cockpit-tools/platform-test/platform-packages/test/index.json`；测试 zip 放在 `platform-packages/test/dist`。
4. 手动构建测试平台包时使用 `.github/workflows/platform-packages.yml` 的 `workflow_dispatch channel=test`；需要真实远端下载时再勾选 `publish_test_branch`。
5. 手动构建测试桌面端时使用 `.github/workflows/build-matrix.yml` 的 `workflow_dispatch channel=test`；需要真实 Tauri updater 验证时再勾选 `publish_test_release`。
6. 连续验证升级提示时，只允许通过 `test_version` 临时生成 `1.0.1-test.1`、`1.0.1-test.2` 等测试版本；禁止把测试版本写进正式 `package.json`、正式 `CHANGELOG` 或正式 release tag。
7. 测试通道可以复用正式签名密钥，但必须保持 endpoint 隔离；正式 `latest.json` 和正式 `platform-packages/index.json` 不得引用 test channel artifact。
8. 后续新平台迁移完成后，必须先通过 test channel 验证 Windows/macOS/Linux 对应 artifact 的安装、卸载、检查更新、更新弹框、更新日志和包大小，再考虑进入正式通道。

## 6. 新平台迁移流程

后续平台迁移按以下顺序执行：

1. 定义平台包 ID、平台 ID、能力列表、页面 contribution 和平台类型。
2. 把原 React 业务组件拆成 `react-remote-esm-v1` remote 入口，禁止手写近似页面替代原页面。
3. 把账号、OAuth、切号、配额、runtime 等业务命令迁到 sidecar adapter。
4. 保留 Core Shell 的稳定 command facade，让 UI 不直接依赖 adapter 进程细节。
5. 编写并校验 `manifest.json` 和 `runtime/index.json`。
6. 构建 `ui/remoteEntry.js`、`ui/style.css` 和当前 OS/arch adapter，并验证 remote UI 产物实际导出 `mount/unmount` 且不残留 `process.env`。
7. 用 `npm run package:platform` 打包 zip，计算真实大小和 `sha256`。
8. 用 `npm run package:platform-index` 汇总各 OS/arch metadata，更新或生成远端 `platform-packages/index.json`。
9. 执行 `npm run verify:platform-packages`，确认预期平台集合、标准打包脚本/CI workflow、manifest、runtime、index、dist zip、artifact size/sha、更新日志、`assets/package-info.json`、remote UI 导出、remote source 复用原业务 content、zip 内容、sidecar adapter crate/workspace/binary、宿主平台包清单、生命周期入口、平台页壳 `runtimeReady` gate 和隐藏入口 gate 一致；隐藏入口审计至少覆盖 Dashboard、SideNav、平台布局弹框、App 路由、自动刷新、账号迁移、数据备份/恢复、浮动卡片、托盘、macOS 原生菜单、token keeper、Web report 和 provider current。
10. 完整热更新总目标验收时必须执行 `npm run audit:platform-full-hot-update`；该命令要求所有平台都是 `sidecarAdapter` 且 `nativeBoundaries=[]`，未通过时只能说明还有平台业务残留在宿主，不能宣称全部迁移完成。
11. 批量导入、批量测试、流式聊天、任务调度等依赖进度事件、取消状态、会话缓存或 `AppHandle.emit` 的命令，迁入 sidecar adapter 前必须先定义 sidecar-to-host 事件桥、轮询状态协议或持久化 session；没有事件/状态协议时必须保留为过渡 `nativeBoundary`，禁止只改 command facade 或只删 boundary。
12. 接入平台页右上角 `PlatformPackageToolbar`、通用不可用页和 `runtimeReady` gate。
13. 接入 Dashboard、托盘/菜单、自动刷新、账号迁移、数据备份/恢复、Web report、provider current、token keeper、浮动卡片和路径重试等隐藏入口 gate。
14. 验证安装、卸载、更新、检查更新、更新日志、包大小、remote UI、adapter 方法和隐藏入口 gate。

## 7. 模板平台

Zed 是第一个模板平台，必须满足：

1. `packageMode=hotUpdate`。
2. `installKind=sidecarAdapter`。
3. `ui.protocol=react-remote-esm-v1`。
4. 宿主 `ZedAccountsPage` 只保留页面壳、平台切换入口、remote tabs slot、右上角平台包操作和 `runtimeReady` gate。
5. Zed 账号、OAuth、切号、配额、runtime 命令通过 adapter facade 执行。
6. 卸载后不显示账号总览、tabs、工具栏、账号卡片和专属弹框。
7. 安装后业务区应与迁移前页面体验保持一致。

Kiro 是第二个模板平台，后续带有多个业务 tab 的平台必须参考 Kiro：

1. 账号总览、实例管理等业务 tab 必须由 remote UI 渲染，宿主只提供 remote tabs slot。
2. 宿主页面只保留页面壳、平台切换入口、右上角平台包操作、通用不可用页和 `runtimeReady` gate。
3. 实例、多开、runtime 等业务命令必须和账号/OAuth/切号一样进入 sidecar adapter，禁止留在宿主 native boundary。
4. 侧边栏、仪表盘、托盘刷新和自动刷新必须 respect `runtimeReady`；未安装时只显示状态和入口，不读取账号、不刷新配额、不启动实例。
5. `manifest.json`、`runtime/index.json`、远端 index 和 adapter methods 必须同步更新。

GitHub Copilot 是第三个模板平台，后续涉及官方 IDE 切号、本地授权同步、实例管理、托盘/菜单和后台 token keeper 的平台必须参考 GitHub Copilot：

1. 账号总览、实例 tab、OAuth、切号、配额、OpenCode 授权同步、runtime 都必须通过 sidecar adapter 提供。
2. 宿主页面只保留页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
3. Dashboard、托盘、macOS 原生菜单、自动刷新、token keeper、路径重试等隐藏入口必须全部 respect `runtimeReady`。
4. 未安装时只显示入口状态，不读取账号、不刷新配额、不同步本地授权、不启动官方 IDE 或实例。
5. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

Windsurf 是第四个模板平台，后续涉及 token 登录、邮箱密码登录、Devin 授权、默认 profile 回写、Web report 或后台保活回写的平台必须参考 Windsurf：

1. token 登录、邮箱密码登录、批量密码导入、Devin 授权、默认 profile 回写、实例管理和 runtime 都必须通过 sidecar adapter 提供。
2. token keeper 不得调用会启动官方 IDE 的切号方法；当前账号回写默认 profile 必须使用不会启动客户端的 adapter 方法，例如 `switch.injectDefaultProfile`。
3. Dashboard、托盘、macOS 原生菜单、自动刷新、token keeper、Web report 和路径重试等隐藏入口必须全部 respect `runtimeReady`。
4. 未安装时只显示入口状态，不读取账号、不刷新配额、不回写官方客户端 profile、不启动 Windsurf 实例。
5. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

Cursor 是第五个模板平台，后续涉及官方 IDE 默认 profile 写入、多开实例和隐藏入口 gate 的平台必须参考 Cursor：

1. 账号总览和实例 tab 必须由 remote UI 渲染，宿主只提供页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
2. 账号、OAuth、切号、配额、实例和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
3. token keeper、Dashboard、托盘、macOS 原生菜单、自动刷新、Web report 和路径重试等隐藏入口必须全部 respect `runtimeReady`。
4. token keeper 的默认 profile 回写必须使用不会启动官方 IDE 的 adapter 方法，例如 `switch.injectDefaultProfile`。
5. 未安装时只显示入口状态，不读取账号、不刷新配额、不回写官方客户端 profile、不启动 Cursor 实例。
6. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

Gemini 是第六个模板平台，后续涉及 CLI home、终端启动命令、OAuth pending restore 或 Web report 的平台必须参考 Gemini：

1. 账号总览和实例 tab 必须由 remote UI 渲染，宿主只提供页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
2. 账号、OAuth、token 登录、切号、配额、实例、launch command 和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
3. `instance.getLaunchCommand` 和 `instance.executeLaunchCommand` 必须保留原 Gemini CLI 终端启动语义；迁移不得把 CLI 命令流改成直接窗口控制。
4. Gemini 默认 profile 写入默认 `~/.gemini`，实例 profile 写入对应 `GEMINI_CLI_HOME/.gemini`；后台保活只能使用不会启动 CLI 的 adapter 方法，例如 `switch.injectDefaultProfile`。
5. 启动恢复、Web report、Dashboard、托盘、macOS 原生菜单、自动刷新和 token keeper 必须全部 respect `runtimeReady`。
6. 未安装时只显示入口状态，不读取账号、不刷新配额、不恢复 OAuth pending 状态、不回写 profile、不启动 Gemini CLI。
7. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

Trae 是第七个模板平台，后续涉及运行中实例保护、严格登录校验或默认客户端注入保护的平台必须参考 Trae：

1. 账号总览和实例 tab 必须由 remote UI 渲染，宿主只提供页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
2. 账号、OAuth、token 登录、本地导入、切号、配额、实例和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
3. Trae token 保活窗口、运行中实例账号保护、`CheckLogin` 严格校验必须封装在 adapter 方法中，例如 `accounts.shouldRefreshToken`、`accounts.refresh`、`accounts.checkLogin`。
4. token keeper 的默认客户端回写必须使用不会启动官方客户端的 adapter 方法，例如 `switch.injectDefaultProfile`；Trae 正在运行时 adapter 必须自动跳过默认 profile 覆盖。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、token keeper、Web report、provider current 和路径重试等隐藏入口必须全部 respect `runtimeReady`。
6. 未安装时只显示入口状态，不读取账号、不刷新配额、不执行 CheckLogin、不回写默认客户端、不启动 Trae 实例。
7. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

Qoder 是第八个模板平台，后续涉及官方 OAuth/OpenAPI 刷新、多开实例、Web report、浮动卡片或路径重试的平台必须参考 Qoder：

1. 账号总览和实例 tab 必须由 remote UI 渲染，宿主只提供页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
2. 账号、OAuth、本地导入、切号、配额、实例和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
3. adapter methods 只允许声明 Qoder 真实实现的方法；不得照搬其它平台的 token 登录、CheckLogin 或 callback URL 方法。
4. Qoder 默认客户端和多开实例写入必须遵守官方客户端真实落盘规则，实例启动前按绑定账号写入对应 user data。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、浮动卡片和路径重试等隐藏入口必须全部 respect `runtimeReady`。
6. 未安装时只显示入口状态，不读取账号、不刷新配额、不回写默认客户端、不启动 Qoder 实例。
7. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

CodeBuddy 是第九个模板平台，后续涉及 VS Code 系客户端 `state.vscdb` 注入、Token 导入、本地导入、设置页账号覆盖、Web report、浮动卡片或路径重试的平台必须参考 CodeBuddy：

1. 账号总览和实例 tab 必须由 remote UI 渲染，宿主只提供页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
2. 账号、OAuth、Token 导入、本地导入、切号、配额、实例和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
3. token keeper 的默认客户端回写必须使用不会启动官方客户端的 adapter 方法，例如 `switch.injectDefaultProfile`。
4. CodeBuddy 默认客户端和多开实例写入必须遵守官方客户端真实落盘规则，实例启动前按绑定账号写入对应 user data 的 `state.vscdb`。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、token keeper、设置页账号覆盖、浮动卡片和路径重试等隐藏入口必须全部 respect `runtimeReady`。
6. 未安装时只显示入口状态，不读取账号、不刷新配额、不回写默认客户端、不启动 CodeBuddy 实例。
7. CodeBuddy 与 CodeBuddy CN 必须作为两个独立平台包迁移；共享源码不等于共享安装态或共享远端发版。
8. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

CodeBuddy CN 是第十个模板平台，后续涉及同一套件多区域版本、共享 UI 但独立安装态、WorkBuddy 同步或区域专属 secret 写入的平台必须参考 CodeBuddy CN：

1. `platformId` 必须使用 `codebuddy_cn`，不得和 CodeBuddy 国际版共享安装态、adapter endpoint、runtimeReady、artifact、版本或更新日志。
2. 账号总览和实例 tab 必须由 remote UI 渲染，宿主只提供页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
3. 账号、OAuth、Token 导入、本地导入、同步到 WorkBuddy、切号、配额、实例和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
4. CodeBuddy CN 默认客户端和多开实例写入必须遵守 CN 官方客户端真实落盘规则，CN session id 使用 `Tencent-Cloud.genie-ide-cn`，CN secret key 使用 `planning-genie.new.accessTokencn`，实例启动前按绑定账号写入对应 user data 的 `state.vscdb`。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、token keeper、设置页账号覆盖、浮动卡片和路径重试等隐藏入口必须全部 respect `runtimeReady`。
6. 未安装时只显示入口状态，不读取账号、不刷新配额、不同步 WorkBuddy、不回写默认客户端、不启动 CodeBuddy CN 实例。
7. 从 CodeBuddy CN 页面触发的 WorkBuddy 同步属于 CodeBuddy CN 包边界；WorkBuddy 页面反向同步在 WorkBuddy 自身迁移前仍属于 WorkBuddy 边界。
8. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、artifact size 和 sha256 必须同步更新。

Claude 是第十一个模板平台，也是第一个从 `coreNativeBoundary` 过渡态升级为完整 `sidecarAdapter` 的高复杂度平台；后续 Desktop/CLI/Gateway/OAuth/实例复合平台必须参考 Claude：

1. `claude_manager` 必须保持 `installKind=sidecarAdapter`，`contributions.nativeBoundaries=[]`，不得再把 Claude business command 留在宿主 native boundary。
2. 账号总览、Claude CLI 和多开实例 tab 必须由 remote UI 渲染；宿主只保留页面壳、平台切换入口、右上角平台包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
3. `cockpit-claude-adapter` 必须覆盖账号、Desktop 登录、CLI 启动命令、Gateway、OAuth、配额、切号、实例、runtime、启动路径探测和启动目标扫描；宿主 command 只允许做安装态 gate、adapter facade、事件转发、托盘刷新和系统级窗口/权限桥接。
4. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、浮动卡片、路径重试、账号迁移和数据备份等隐藏入口必须 respect `runtimeReady`，并通过 Claude adapter 获取业务数据。
5. 托盘、macOS 原生菜单和设置页启动路径入口禁止重新直接引用宿主 `claude_account`、`claude_desktop_gateway` 或 `claude_instance` 模块。
6. Desktop 登录进度、验证窗口或其它需要主应用事件能力的交互，必须通过 adapter-to-host 事件桥扩展，禁止为了事件便利把业务流程搬回宿主。
7. Codex 平台包必须携带 adapter 运行时依赖的二级 helper/sidecar；当前 API 服务依赖 `cockpit-cliproxy`，必须与 `cockpit-codex-adapter` 一起按 OS/arch 打进包内，并由 contract/平台包校验检查存在性和可执行权限。
8. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、capabilities、contributions、artifact size 和 sha256 必须同步更新。

WorkBuddy 是第十二个模板平台，后续涉及同套件独立平台包、共享 auth 文件读写、签到、反向同步或默认客户端共享登录态的平台必须参考 WorkBuddy：

1. `platformId` 必须使用 `workbuddy`，不得和 CodeBuddy CN 共享安装态、adapter endpoint、runtimeReady、artifact、版本或更新日志。
2. 账号总览和多开实例 tab 必须由 WorkBuddy remote UI 渲染；宿主只提供页面壳、平台切换入口、右上角包操作、remote tabs slot、通用不可用页和 `runtimeReady` gate。
3. 账号、OAuth、Token 导入、本地导入、同步到 CodeBuddy CN、签到、切号、配额、实例和 runtime 都必须通过 sidecar adapter 提供；宿主 command 只允许做 gate、事件转发、托盘刷新和 path missing 事件桥接。
4. WorkBuddy 默认客户端写入必须遵守当前真实落盘规则：默认数据目录为 `~/.workbuddy/app`，登录态写入 `CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`，切号或实例启动前按绑定账号回写共享 auth 文件。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、token keeper、设置页账号覆盖、浮动卡片和路径重试等隐藏入口必须全部 respect WorkBuddy `runtimeReady`。
6. 未安装时只显示入口状态，不读取 WorkBuddy 账号、不刷新配额、不签到、不同步 CodeBuddy CN、不回写默认客户端、不启动 WorkBuddy 实例。
7. WorkBuddy 与 CodeBuddy CN 同套件但必须独立迁移；共享 UI 或类型只能作为源码复用，安装态、adapter、runtimeReady、artifact、版本和更新日志不得互相代替。
8. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、capabilities、contributions、artifact size 和 sha256 必须同步更新。

Codex 是第十三个模板平台，也是第一个覆盖账号、API 服务、本地网关、模型供应商、唤醒任务、会话管理、线程同步和多开实例的大体量完整 `sidecarAdapter` 平台；后续本地网关、模型供应商、唤醒、会话和实例复合平台必须参考 Codex：

1. `codex` 必须保持 `installKind=sidecarAdapter`，`contributions.nativeBoundaries=[]`，不得再把 Codex 账号、OAuth、API 服务、本地网关、模型供应商、唤醒、会话、线程同步或多开实例命令留在宿主 native boundary。
2. Codex 账号总览、模型供应商、唤醒任务、多开实例和会话管理 tabs 必须由 Codex remote UI 渲染；宿主只保留平台包生命周期、右上角包操作、通用不可用页、remote tabs slot 和 `runtimeReady` gate。
3. `cockpit-codex-adapter` 必须覆盖账号读写、导入导出、批量导入、切号、OAuth、配额、配置/速度、本地 API 服务、本地网关、模型供应商、provider gateway、唤醒任务、通用 wakeup 调度、wakeup verification、会话/线程同步、会话可见性修复、token 统计、废纸篓、多开实例、启动命令、实例启动、平台设置和 runtime 相关业务。
4. 宿主 command 只允许保留安装态 gate、adapter facade、事件转发、托盘刷新、path missing 事件和系统级 opener/终端/窗口权限桥接；类似 `open_codex_config_toml` 的命令必须由 adapter 解析业务路径，宿主只执行通用系统打开动作。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、token keeper、浮动卡片、路径重试、账号迁移、数据备份/恢复和重启前本地网关处理等隐藏入口必须全部 respect Codex `runtimeReady`，并通过 Codex adapter 获取业务数据或执行业务动作。
6. 未安装时只显示入口状态，不读取 Codex 账号、不刷新配额、不恢复 OAuth、不启动本地 API 服务、不运行唤醒任务、不读取会话、不回写 `config.toml`、不启动 Codex 实例。
7. `manifest.json`、`runtime/index.json`、远端 index、adapter methods、capabilities、contributions、artifact size 和 sha256 必须同步更新。

Antigravity 系列是第十四个模板平台，也是第一个双 runtime target 套件迁移模板；后续同一分组下有多个官方客户端或运行目标的平台必须参考 Antigravity：

1. `antigravity` 与 `antigravity_ide` 必须是两个独立平台包，安装态、runtimeReady、artifact、版本、更新日志、包大小和 native boundary 不得互相代替。
2. 两个包可以共享 remote UI 源码，但必须分别构建 `ui/remoteEntry.js` 和 `ui/style.css`，分别写入自己的 manifest/runtime/index。
3. 账号总览、多开实例、唤醒任务和账户检测 tabs 必须由 remote UI 渲染；宿主只保留页面壳、平台分组切换、remote tabs slot、右上角包操作、通用不可用页和 `runtimeReady` gate。
4. Antigravity 系列当前完成标准是两个包都保持 `installKind=sidecarAdapter` 且 `nativeBoundaries=[]`；账号读写、手动 token 导入、插件凭据同步、导入导出、切号、配额刷新、runtime、多开实例、OAuth、唤醒和账户检测都必须通过对应 adapter methods 提供，禁止再回退为宿主 native boundary。
5. Dashboard、托盘、macOS 原生菜单、自动刷新、Web report、provider current、浮动卡片、路径重试、账号迁移和数据备份/恢复等隐藏入口都必须 respect 对应包的 `runtimeReady`，并通过 adapter 或通用 Host API 获取业务数据或执行业务动作。
6. 宿主 command 只允许做安装态 gate、adapter facade、Host Event Bridge、托盘刷新、path missing 事件和系统级 opener/窗口权限桥接；不得直接读取 Antigravity 业务数据。
7. 应用启动恢复、唤醒任务调度等后台生命周期必须通过包内 `runtime.restore` 或等价 adapter 方法执行；Core Shell 禁止直接启动 Antigravity 宿主 scheduler 或读取旧账号存储。
7. 未安装或 `runtimeReady=false` 时，只允许显示入口状态和通用不可用页；不得加载 remote UI、读取账号、刷新配额、启动 OAuth、执行切号、启动实例、运行唤醒或账户检测。
8. 两个包的 `manifest.json`、`runtime/index.json`、远端 index、adapter methods、capabilities、contributions、artifact size 和 sha256 必须同步更新；使用 `--update-index` 打包时必须串行执行，禁止多个平台包并行写同一个 `platform-packages/index.json`。

## 8. 必跑验证

平台热更新架构或平台迁移改动完成前至少执行：

```bash
npm run build:platform-ui -- antigravity
npm run build:platform-ui -- antigravity_ide
npm run build:platform-ui -- zed
npm run build:platform-ui -- kiro
npm run build:platform-ui -- github-copilot
npm run build:platform-ui -- windsurf
npm run build:platform-ui -- cursor
npm run build:platform-ui -- gemini
npm run build:platform-ui -- trae
npm run build:platform-ui -- qoder
npm run build:platform-ui -- codebuddy
npm run build:platform-ui -- codebuddy_cn
npm run build:platform-ui -- claude_manager
npm run build:platform-ui -- workbuddy
npm run build:platform-ui -- codex
npm run verify:platform-packages
npm run audit:platform-full-hot-update
npm run typecheck
node scripts/check_locales.cjs
cargo test --manifest-path src-tauri/Cargo.toml platform_package --lib
cargo check --manifest-path src-tauri/Cargo.toml
cargo check -p cockpit-zed-adapter
cargo check -p cockpit-kiro-adapter
cargo check -p cockpit-github-copilot-adapter
cargo check -p cockpit-windsurf-adapter
cargo check -p cockpit-cursor-adapter
cargo check -p cockpit-gemini-adapter
cargo check -p cockpit-trae-adapter
cargo check -p cockpit-qoder-adapter
cargo check -p cockpit-codebuddy-adapter
cargo check -p cockpit-codebuddy-cn-adapter
cargo check -p cockpit-claude-adapter
cargo check -p cockpit-workbuddy-adapter
git diff --check
```

`npm run audit:platform-full-hot-update` 是完整热更新总目标的缺口审计。当前所有平台都必须是 `sidecarAdapter` 且 `nativeBoundaries=[]`，因此该命令必须通过；任何新增平台或回归到 `coreNativeBoundary` 的平台都必须让该命令失败并列出缺口。

strict 审计失败时还必须输出 native boundary 明细，并按 `accounts`、`gateway-provider`、`wakeup`、`sessions`、`instances-runtime`、`quota-billing`、`import-export` 等业务域归类。后续迁移 Codex 不能只减少数量，必须把命令迁到 adapter、同步删除 manifest/runtime/index 中对应 boundary，并用 strict 审计确认数量和业务域明细收敛；不得只改宿主 command facade 而保留 boundary，也不得只删 boundary 而没有 adapter method 与包内二进制。
