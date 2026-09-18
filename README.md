# 流转平台

局域网内部使用的流转与协同系统。一台 Windows 机器当服务端，其他人用浏览器访问。

- **桌面端**：双击运行，显示服务状态、一键复制访问地址、托盘常驻
- **服务端**：Rust + Axum，内嵌网页与 SQLite，单文件、零外部依赖
- **网页端**：局域网用户登录使用，支持流转、导入、统计、权限与消息提示

## 目录结构

```
crates/
  server/    Axum 应用：API + 内嵌前端资源 + SQLite，可独立运行
  desktop/   Tauri 外壳：服务控制台、托盘、开机自启、备份
apps/
  web/       前端（Vite + React + Ant Design）
             index.html   局域网用户端
             console.html 桌面控制台
scripts/     图标生成等工具脚本
```

`crates/server` 不依赖 Tauri，所以后端和网页在 macOS 上就能完整开发和调试，
只有托盘、剪贴板、开机自启这几个桌面能力需要跑 Tauri 才能验证。

## 开发

```bash
npm install --include=dev     # 见下方「环境注意事项」
cargo run -p circulation-server   # 后端 + 网页，浏览器打开 http://127.0.0.1:8080
npm run dev:web                   # 前端热更（已配好到 8080 的接口代理）
npm run desktop:dev               # 完整桌面端
```

默认管理员账号 `admin` / `admin123`，首次登录会强制改密。
可用 `CIRCULATION_ADMIN_PASSWORD` 环境变量指定初始密码。

服务端支持的环境变量：

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `CIRCULATION_HOST` | `0.0.0.0` | 监听地址 |
| `CIRCULATION_PORT` | `8080` | 监听端口 |
| `CIRCULATION_DATA_DIR` | `./data` | 数据目录 |

## 测试

```bash
cargo test --workspace       # 120 个测试：权限、导入、流转、备份、API 集成
cargo clippy --workspace --all-targets
npm run typecheck:web        # 前端类型检查
node scripts/ui-shot.mjs     # 无头浏览器跑一遍真实流程并逐页截图（需先启动服务）
```

`scripts/ui-shot.mjs` 会登录、走完强制改密、创建字段定义、把一批数据从粘贴到导入跑完，
逐页截图到 `/tmp/ui-shots`，并汇总浏览器控制台报错。改动界面后跑一次，比肉眼看代码可靠。

## 打包 Windows 安装包

```bash
npm run desktop:build
```

`.msi` 只能在 Windows 上打，所以只做 NSIS 安装包。推荐在 Windows 机器或
GitHub Actions 的 `windows-latest` 上执行；Mac 上交叉编译也是官方支持的路径：

```bash
brew install nsis llvm && rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
cd crates/desktop
npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
# 产物在 target/x86_64-pc-windows-msvc/release/bundle/nsis/
```

## 环境注意事项

**`NODE_ENV=production` 会导致 npm 跳过全部 devDependencies**，表现是
`@tauri-apps/cli`、`vite`、`typescript` 都装不上，`node_modules/.bin` 甚至不会生成。
安装时显式带上：

```bash
npm install --include=dev
```

**首次运行 Windows 防火墙会弹窗**，必须勾选「专用网络」并允许，
否则同事打不开地址。控制台里对这个有提示。

**WebView2**：Windows 10（1809 以上）和 Windows 11 自带，一般无需处理。
若目标机器完全无外网，把 `tauri.conf.json` 里的
`bundle.windows.webviewInstallMode` 改成 `offlineInstaller`（安装包会大约 127MB）。

## 当前进度

已完成：

- 数据层与迁移（30 张表，迁移内嵌进二进制且可重复执行）
- 用户、角色、权限点体系，登录会话与审计日志
- 桌面端服务控制台：状态、地址复制、二维码、托盘、开机自启、备份、日志
- 网页端：登录、强制改密、按权限渲染的主框架、用户管理、角色权限配置
- 消息提醒：站内消息、实时推送 + 兜底轮询、未读徽标、提示音
- 统计：全局看板（进行中/超期/今日新增办结、按天趋势、各环节情况、按人工作量）
  与个人视角（我的待办/已办/超期/平均处理时长）；超期阈值和时间窗口可切换
- 字段定义与数据导入：字段/CSV/Excel 拖拽或粘贴 → 表头识别 → 列映射（可存模板复用）
  → 校验预览 → 去重提交；记录列表、搜索、编辑、删除、导出 Excel
- 流转引擎：可配置的步骤链（发起 → 分派 → 承办 → 确认），支持多人承办、
  「全部完成 / 任一人完成」规则、确认打回、按条件跳转；流转中心（待办 / 已办 /
  我发起的 / 我参与的）、流程详情时间线、流程配置页

待做：Windows 打包。

### 消息提醒是怎么送的

- **优先服务端推送**：`GET /api/events`（SSE）推一个「有变化」的信号，前端收到就
  重新拉一次自己的数据，正常情况下秒级到达。信号里刻意不带内容，也不带收件人——
  按人算推送目标要把受影响的用户 id 一路从业务层传出来，绕一大圈；30 人规模下
  让每个客户端收到信号各拉各的，代价可以忽略，而且断线期间漏掉的推送不影响正确性。
- **兜底轮询始终在跑**：推送正常时 60 秒一次，只作为「万一推送静默失效」的保险。
  内网代理缓冲长连接是常见情况，有这层兜底就不会漏消息。
- **连不上就自动降级**：SSE 连续失败 3 次就关掉通道，轮询提到 15 秒一次。触发条件
  是「真连不上」，而不是「浏览器不支持」——WebView2、Chrome、Edge、Firefox、
  Safari 对 SSE 和 WebSocket 的支持都很完整，按特性探测降级基本不会触发。
- 为什么用 SSE 而不是 WebSocket：这里只有服务端推客户端这一个方向，SSE 就是普通
  HTTP，`EventSource` 自带断线重连；WebSocket 的双向能力用不上，重连、心跳、
  退避都得自己写。两者在「每个标签页一条长连接」这点上没有区别。
- 页面被导航销毁时会主动断开推送连接（`beforeunload` / `pagehide`）。
  少了这一步，反复刷新或切换账号后连接会堆在浏览器连接池里，后续请求被排队。

### 统计口径上的几个取舍

- **超期看的是「当前环节挂了多久」**，不是流程创建了多久。一个流程整体跑了三天，
  但每一环都是当天办的，不该算超期。阈值默认 48 小时，界面上可以切。
- **时间边界在 Rust 侧算好再当字符串比较**。库里存的是本地时间，而 SQLite 的
  `now` 是 UTC，直接用 `julianday('now')` 会差出一个时区。时长换算仍用 `julianday`，
  因为两边都是库里的本地时间，差值不受影响。
- **趋势图用柱状而不是折线**。按天计数是离散的，画成连续曲线会暗示中间的空白也有
  量；而且只有一两天有数据时，柱子看得见，折线会贴在右边缘看不出来。
  没有数据的日子会补零，否则折线/柱状图会断。
- **个人统计不需要权限**，全局统计要 `stats:view`。内置角色默认都带这个权限
  （小团队里工作量透明是好事），但管理员可以通过改角色把它收掉。

### 发布安装包

`.github/workflows/build.yml` 会在**打 tag 时**自动构建两个平台的安装包：

| 平台 | 产物 | 说明 |
| --- | --- | --- |
| Windows | `.exe`（NSIS 安装包） | 双击安装，可改安装目录，自动建桌面快捷方式 |
| macOS | `.dmg` | 打开后拖进「应用程序」 |

打 tag 触发构建：

```bash
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

构建完成后会生成一个**草稿 Release**，产物挂在上面。确认无误后到 Releases 页面
手动发布即可。PR 上只跑测试与 clippy，不打包。

**为什么 Windows 包在 CI 上打而不是在 Mac 上交叉编译**：Tauri 官方支持
`cargo-xwin` 从 macOS 交叉编译 Windows 产物，但文档明确说这是「本地虚拟机或 CI
都不可用时才考虑」的下策，NSIS 打包在非 Windows 主机上容易出意外。CI 上的
`windows-latest` 是原生环境，最省心。

**安装包未做代码签名**，首次打开系统会拦一下：

- Windows：SmartScreen 提示 → 「更多信息」→「仍要运行」
- macOS：右键点应用选「打开」，或执行
  `xattr -dr com.apple.quarantine "/Applications/流转平台.app"`

内网自用可以直接这么放行；要消除提示需要买代码签名证书，Windows 和 macOS 各一份。

### 调试工具

- `node scripts/ui-shot.mjs` 用无头浏览器把主要流程真跑一遍并逐页截图，
  失败时会自动截图并打印当时的按钮和接口探测结果。
- `node scripts/diagnose-connections.mjs` 反复导航并统计到服务端的连接数，
  用来排查长连接堆积类的浏览器侧问题。

### 流转引擎的一些约定

- **发起步骤不产生待办**。实例创建后引擎立刻从 `start` 推进，第一步真正的待办落在
  分派环节，这样「1 发起 → 到 2」的语义才和实际待办一致。
- **分派步骤负责决定下一步谁来做**。分派人提交时带上要指派的人员 id，存在任务的
  `result` 里，下一步配置成 `assigned` 就会读取这批人。指派为空会直接报错拦下，
  不会产生一个没人认领的流程。
- **停在原地和流程结束是两回事**。多人承办时，一个人做完但还有人没做，时间线上
  不写目标步骤；只有真正推进或结束才记目标。这两种情况都用「没有下一步」表示，
  早先合并处理过，导致时间线上出现误导性的「→ 结束」。
- **打回不删历史**。确认环节打回会在目标步骤新建任务，原有的已完成任务保留，
  时间线能看到完整的往返过程。
- 步骤标识、跳转目标、打回目标在保存时都会校验，指向不存在的步骤会直接拒绝保存。

### 导入的一些设计取舍

- 列映射按表头指纹存成模板，同一个系统的导出文件第二次是「一键导入」
- 表头所在行可手动指定，因为不少系统导出的表格前面有标题行
- 唯一键全局只允许一个；没有配唯一键时只能「全部新增」，避免误判重复把数据冲掉
- 唯一键在同一次导入里重复的行会标错而不是静默覆盖
- 导出的 CSV 带 UTF-8 BOM，否则 Excel 打开中文是乱码
