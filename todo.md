# PDFind 待办事项

## P2P 搜索 PDF 资源功能（多协议）

### 功能概述
在 PDFind 中集成多协议 P2P 网络搜索能力，允许用户直接搜索 PDF 资源并通过协议链接调用下载器下载。

**支持协议**：
- **Phase 1**: eD2k（eMule 网络）
- **Phase 2**: DC++/ADC（Direct Connect 网络）
- **Phase 3**: BitTorrent 索引站（磁力搜索）

### 前端改动 (App.vue)

#### 1. 搜索范围切换器
- **位置**：工具栏上方
- **形式**：分段开关 `[本地] [网络]`
- **持久化**：localStorage 存储，默认本地
- **行为**：
  - 本地模式：现有功能完全不变（路径/选文件/选目录/搜索/OCR搜索/目录树）
  - 网络模式：
    - 隐藏路径栏与选文件按钮
    - 隐藏 OCR 搜索按钮
    - 目录树面板淡化（仅视觉提示，功能保留）
    - 仅显示：关键词输入 + 搜索按钮 + 取消按钮
    - 新增协议选择器（下拉菜单）：`[eD2k] [DC++] [BT索引]`

#### 2. 搜索逻辑分流
- 搜索按钮根据当前范围和协议调用不同命令：
  - 本地 → 现有 `search_file` / `start_search`
  - 网络 + eD2k → `ed2k_search`
  - 网络 + DC++ → `dc_search`
  - 网络 + BT索引 → `bt_search`
- 事件流复用现有模式：
  - `p2p:progress`（服务器连接状态）
  - `p2p:result`（搜索结果）
  - `p2p:done`（完成）
- 取消按钮走同一 `cancel_search` 机制
- 关键词历史两种范围共用

#### 3. 网络结果列表组件 (P2pResults.vue)
- **字段**：
  - 文件名
  - 大小（人性化格式：KB/MB/GB）
  - 来源数（sources/seeds）
  - 协议类型（eD2k/DC++/BT）
- **排序**：默认按来源数降序
- **去重**：按 hash 合并（ed2k hash / infohash / DC++ file hash），来源数累加
- **操作**：
  - **复制链接**：
    - eD2k: `ed2k://|file|名称|大小|hash|/`
    - DC++: `magnet:?xt=urn:tree:tiger:HASH&xl=SIZE&dn=NAME`（或自定义 dc:// 协议）
    - BT: `magnet:?xt=urn:btih:INFOHASH&dn=NAME`
    - 行内显示"已复制"反馈
  - **下载**：调用系统协议处理器
    - ed2k:// → eMule/迅雷/easyMule
    - magnet: → qBittorrent/µTorrent/迅雷
    - dc:// → DC++ 客户端（如 AirDC++）

### 后端改动

---

## Phase 1: eD2k 协议 (src-tauri/src/ed2k/)

#### 1. packet.rs - eD2k 协议包编解码
- 包头格式：`[协议标记 u8][长度 u32 LE][opcode u8]`
- Tag 结构：`type/length/value` 扩展字段
- 实现：
  - 读取/写入包头
  - 读取/写入 tag 列表
  - 参考：andrey23127/ed2k-server（Rust 服务器源码）

#### 2. server_list.rs - 服务器列表管理
- **内置种子列表**（~9 台可信服务器）：
  - eMule Sunrise (176.123.5.89:4725)
  - Nordic Server (77.42.68.79:4232)
  - Sharing-Devils No.2 (85.121.5.137:4232)
  - MO-Server (91.208.162.182:4232)
  - Sharing-Devils No.4 (91.208.162.87:4232)
  - ed2k-rust (85.17.116.222:6082)
  - eMule Cosmic (212.95.35.240:4232)
  - Mazinga Server (213.141.198.207:4232)
  - Drunken Donkey (193.187.90.12:4661)
- **自动更新**：
  - 从 emule-security.org / shortypower.org 拉取 server.met
  - 解析二进制 server.met 格式（u32 count + 每服务器 ip u32 + port u16 + tags）
  - 缓存于应用配置目录（`app_cache_dir()/ed2k-servers.json`）
  - 失败时回退到内置种子列表
- **依赖**：`ureq`（轻量同步 HTTP 客户端，~200KB）

#### 3. client.rs - 单服务器搜索会话
- **流程**：
  1. TCP 连接到服务器（超时 5s）
  2. 发送登录请求（OP_LOGINREQUEST，opcode 0x01）
     - 包含：用户 hash、IP、端口、标签列表（客户端名称/版本）
  3. 接收 ID 变更（OP_IDCHANGE，opcode 0x05）
  4. 发送搜索请求（OP_SEARCHREQUEST）
     - 搜索树：AND(关键词, format=pdf 元标签)
     - 仅搜索 PDF 文件
  5. 接收搜索结果（OP_SEARCHRESULT）
     - 解析标签：FT_FILENAME、FT_FILESIZE、FT_SOURCES、FT_FILEHASH
     - 生成 ed2k 链接：`ed2k://|file|名称|大小|hash|/`
  6. 超时 8-10s 后断开连接
- **错误处理**：连接失败/超时/协议错误静默跳过，不影响整体搜索

#### 4. commands.rs 扩展 - ed2k_search 命令
- **参数**：`keyword: String`
- **流程**：
  1. 加载服务器列表（缓存或拉取更新）
  2. 并发查询 ~6 台服务器（tokio 任务）
  3. 每台服务器独立超时 10s
  4. 结果按 hash 合并去重（来源数累加）
  5. 事件流推送：
     - `p2p:progress`：当前连接的服务器状态
     - `p2p:result`：新结果（单个或批量）
     - `p2p:done`：完成（含统计：成功/失败服务器数、总结果数）
  6. SearchState 取消机制：逐服务器检查取消标志
- **返回**：无（结果通过事件流推送）



---

## Phase 2: DC++/ADC 协议 (src-tauri/src/dc/)

#### 1. hub_list.rs - Hub 列表管理
- **内置种子列表**（公开学术/电子书 hub）：
  - 从 dchublist.org / tehome.ca 等公共 hub 列表拉取
  - 解析 HTML/JSON 格式（协议文档有标准格式）
  - 缓存于 `app_cache_dir()/dc-hubs.json`
  - 按用户数/文件数排序，优先选择大 hub
- **依赖**：复用 `ureq`

#### 2. client.rs - 单 Hub 搜索会话
- **协议**：ADC（Advanced Direct Connect，文本协议，规格书 2025-05 更新）
- **流程**：
  1. TCP 连接到 hub（超时 5s）
  2. 发送 SUP 命令（声明支持的特性）
  3. 发送 INF 命令（客户端信息：昵称/版本/共享大小）
  4. 接收 INF 响应（hub 信息）
  5. 发送 STA 命令（状态）
  6. 发送 SCH 命令（搜索）：`BSCH +keyword ext:pdf`
     - 广播搜索，hub 转发给所有用户
     - 用户回复 RES 命令（文件名/大小/hash/路径）
  7. 收集 RES 响应（超时 8-10s）
     - 解析：`BRES <size> <hash> <path> <name>`
     - 生成 magnet 链接：`magnet:?xt=urn:tree:tiger:HASH&xl=SIZE&dn=NAME`
  8. 发送 QUI 命令（退出）
- **错误处理**：连接失败/超时/协议错误静默跳过

#### 3. commands.rs 扩展 - dc_search 命令
- **参数**：`keyword: String`
- **流程**：
  1. 加载 hub 列表（缓存或拉取更新）
  2. 并发查询 ~3 个 hub（tokio 任务）
  3. 每个 hub 独立超时 10s
  4. 结果按 hash 合并去重
  5. 事件流推送（同 eD2k）
  6. SearchState 取消机制
- **返回**：无

---

## Phase 3: BitTorrent 索引站 (src-tauri/src/bt/)

#### 1. indexer.rs - 索引站 API 客户端
- **支持的索引站**（选择 2-3 个稳定站点）：
  - BTdig (btdig.com) - 最大的 DHT 搜索引擎
  - BTDigg (btdigg.org) - 镜像
  - 或自建爬虫（爬取公开 tracker 的 DHT 索引）
- **API 格式**：
  - BTdig: HTTP GET `https://btdig.com/search?q=KEYWORD` → 解析 HTML 或 JSON API
  - 返回：文件名/大小/seeds/leechers/infohash
- **依赖**：`scraper`（HTML 解析）或 `serde_json`（JSON API）

#### 2. client.rs - 搜索与结果解析
- **流程**：
  1. HTTP GET 请求索引站（超时 10s）
  2. 解析响应（HTML 或 JSON）
  3. 提取结果：
     - 文件名
     - 大小
     - seeds/leechers
     - infohash（40 字符 hex）
     - tracker 列表（可选）
  4. 生成 magnet 链接：`magnet:?xt=urn:btih:INFOHASH&dn=NAME&tr=TRACKER`
- **过滤**：仅保留文件名包含 `.pdf` 的结果（或索引站已过滤）
- **错误处理**：HTTP 错误/解析失败静默跳过

#### 3. commands.rs 扩展 - bt_search 命令
- **参数**：`keyword: String`
- **流程**：
  1. 并发查询 2-3 个索引站（tokio 任务）
  2. 每个站点独立超时 10s
  3. 结果按 infohash 合并去重（seeds 累加）
  4. 按 seeds 数降序排序
  5. 事件流推送（同 eD2k）
  6. SearchState 取消机制
- **返回**：无

---

## 依赖增量

### Rust (Cargo.toml)
```toml
ureq = "2"              # 轻量 HTTP 客户端（eD2k server.met / DC++ hub list / BT 索引站）
scraper = "0.18"        # HTML 解析（BT 索引站 HTML 响应）
```

### 前端 (package.json)
```json
"@tauri-apps/plugin-clipboard-manager": "^2.0.0"  # 复制链接
```

### Tauri Capabilities (src-tauri/capabilities/default.json)
- 添加 `clipboard-manager:default`
- 放宽 opener 插件 URL 校验：允许 `ed2k://`、`magnet:`、`dc://` 协议

---

## 验证计划

### 1. 单元测试
- ed2k/packet.rs：包头/tag 编解码往返测试
- ed2k/server_list.rs：server.met 二进制解析测试
- dc/client.rs：ADC 命令解析测试
- bt/indexer.rs：HTML/JSON 解析测试

### 2. 集成测试 (dev 模式)
- 范围切「网络」→ 协议选 eD2k → 搜常见词（如 `民法典 pdf`）
  - 验证：eMule Sunrise 等服务器返回结果
- 协议选 DC++ → 搜 `python tutorial pdf`
  - 验证：学术 hub 返回结果
- 协议选 BT索引 → 搜 `machine learning pdf`
  - 验证：BTdig 等站点返回结果
- 所有协议：结果列表正常显示（名称/大小/来源数/协议类型）
- 复制链接功能正常（ed2k:// / magnet: / dc://）
- 唤起下载器功能正常（需用户已安装对应客户端）
- 按来源数排序、按 hash 去重

### 3. 容错测试
- 部分服务器/hub/索引站不可达（模拟网络问题）
- 验证：整体搜索仍完成，仅显示成功来源的结果

### 4. 取消测试
- P2P 搜索进行中点击取消
- 验证：按钮正常复位，搜索停止

---

## 明确不做
- ❌ 下载功能（由用户下载器完成）
- ❌ KAD 无服务器网络搜索（eD2k 的 DHT 替代，复杂度高）
- ❌ BitTorrent DHT 爬虫（需实现完整 DHT 协议，复杂度高）
- ❌ 协议混淆（公共服务器标准协议即可）
- ❌ 服务器/hub 管理界面（Phase 2 再议）
- ❌ ipfilter 反假服务器过滤（Phase 2 再议）

---

## 实施预估

### Phase 1: eD2k
- 后端：~600 行 Rust
- 前端：~250 行 Vue/TS
- 预计：2-3 个实施回合

### Phase 2: DC++
- 后端：~400 行 Rust（ADC 文本协议比 eD2k 简单）
- 前端：~100 行（复用 eD2k 前端，仅加协议选择）
- 预计：1-2 个实施回合

### Phase 3: BT 索引
- 后端：~300 行 Rust（HTTP 客户端 + HTML/JSON 解析）
- 前端：~50 行（复用前端）
- 预计：1 个实施回合

**总计**：~1300 行 Rust + ~400 行前端，4-6 个实施回合

---

## 风险与缓解

### 1. 协议细节需对照实现
- **缓解**：
  - eD2k：参考 andrey23127/ed2k-server（Rust 服务器源码）与 eMule 源码
  - DC++：参考 DC++ 官方 ADC 规格书（2025-05 更新）与 nicotine+ 源码
  - BT：参考 BTdig API 文档或爬虫实现

### 2. 国内网络可达性
- **缓解**：
  - eD2k：多服务器并行 + 超时容错
  - DC++：hub 列表按地理位置排序，优先国内 hub
  - BT：索引站可能需要代理，提供代理配置选项（可选）

### 3. 假服务器/恶意 hub
- **缓解**：
  - eD2k：使用可信种子列表（emule-security.org 维护）
  - DC++：使用 dchublist.org 验证的 hub 列表
  - BT：索引站信誉评估（用户评分/站长验证）

### 4. 索引站反爬
- **缓解**：
  - BT：User-Agent 伪装、请求间隔、多站点轮换
  - 备用方案：提供手动输入 magnet 链接功能

---

## 桌面多平台云构建（GitHub Actions）

### 功能概述
通过 GitHub Actions 自动构建 macOS（universal dmg）与 Linux（x64 + arm64 的 deb/rpm）安装包，推送 `v*` 标签自动创建 GitHub Release 并附加全部产物。Windows 包仍本地手动构建后追加到同一 Release。

### 代码改动（通用，Windows 零影响）

#### 1. src-tauri/src/pdfx.rs - pdfium 库名按平台条件编译
```rust
#[cfg(windows)]             const PDFIUM_LIB: &str = "pdfium.dll";
#[cfg(target_os = "macos")] const PDFIUM_LIB: &str = "libpdfium.dylib";
#[cfg(target_os = "linux")] const PDFIUM_LIB: &str = "libpdfium.so";
```
- 替换 `dll_candidates()` 中 4 处硬编码 `"pdfium.dll"`
- 候选路径逻辑不变（资源目录 > exe 同目录 > 开发目录），仅文件名换常量

#### 2. src-tauri/tauri.conf.json - resources 改 glob
```json
"resources": { "binaries/*": "./" }
```
- 各平台构建机只放本平台库，互不干扰

#### 3. README.md - 补 macOS/Linux 使用说明
- macOS：「未知开发者」右键打开，或 `xattr -cr /Applications/PDFind.app`
- Linux：`sudo dpkg -i PDFind_x.x.x_amd64.deb` / `sudo rpm -i PDFind-*.rpm`

### 新增 .github/workflows/build-release.yml（4 构建 job + 1 发布 job）

#### 触发
- `push: tags: ['v*']` + `workflow_dispatch`（先手动验证再打 tag）

#### job 1: build-mac（macos-latest）
1. checkout + Node 20 + Rust stable（targets: `aarch64-apple-darwin,x86_64-apple-darwin`）+ rust-cache
2. `npm install`
3. 下载 `pdfium-mac-univ.tgz`（bblanchon/pdfium-binaries latest）→ `lib/libpdfium.dylib` 移入 `src-tauri/binaries/`
4. **`codesign --force --sign -`** 给 dylib 补 ad-hoc 签名（Apple Silicon 硬性要求，否则启动 killed:9）
5. `npx tauri build --target universal-apple-darwin`
6. 上传 artifact：`PDFind_x.x.x_universal.dmg`

#### job 2: build-linux-x64（ubuntu-latest）
1. 装系统依赖：`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf`
2. 下载 `pdfium-linux-x64.tgz` → `lib/libpdfium.so` 移入 `src-tauri/binaries/`
3. `npx tauri build --bundles deb,rpm`
4. 上传 artifacts：`PDFind_x.x.x_amd64.deb` + `PDFind-x.x.x.x86_64.rpm`

#### job 3: build-linux-arm64（ubuntu-24.04-arm）
- **原生构建**（公开仓库免费 GA，4 vCPU Neoverse N2，速度与 x64 相当）
- runner 标签必须精确写 `ubuntu-24.04-arm`（无 `ubuntu-latest-arm`）
- 步骤同 job 2，pdfium 换 `pdfium-linux-arm64.tgz`
- 上传 artifacts：`PDFind_x.x.x_arm64.deb` + `PDFind-x.x.x.aarch64.rpm`
- Linux（含 arm）无签名/codesign 要求

#### job 4: release（needs: [mac, linux-x64, linux-arm64]，仅 tag 触发）
- `permissions: contents: write`
- `softprops/action-gh-release` 统一创建 Release，附 dmg + 4 个 Linux 包
- 单一 job 统一发版，避免多 workflow 竞争创建 Release

### 发版流程（v0.0.3 起）
```
本地: 三处版本号 +0.0.3 → commit → tag v0.0.3 → push (含 tag)
CI:   自动构建 mac dmg + linux x64/arm64 deb/rpm → 自动创建 Release 附全部产物
本地: Windows zip 照旧手动构建，追加到同一 Release
```

### 产物清单（每次发版）
| 平台 | 包名 |
|------|------|
| macOS (M 系列 + Intel) | `PDFind_x.x.x_universal.dmg` |
| Linux x64 | `PDFind_x.x.x_amd64.deb` / `PDFind-x.x.x.x86_64.rpm` |
| Linux arm64 | `PDFind_x.x.x_arm64.deb` / `PDFind-x.x.x.aarch64.rpm` |
| Windows x64 | `PDFind-vX.X.X-win64.zip`（本地手动） |

### 风险与对策
1. **mac dylib 未签名被拒载** → CI 步骤 4 已含 ad-hoc codesign
2. **未 notarize，首开提示「未知开发者」** → README 注明右键打开 / `xattr -cr`；正式分发需 Apple 开发者账号（$99/年）
3. **universal/deb/rpm 构建失败** → workflow_dispatch 先手动验证，通过后再打 tag
4. **webkit2gtk 版本兼容** → 锁 ubuntu-24.04 系（webkit2gtk-4.1）
5. **arm runner 仅公开仓库免费** → 本仓库满足；转私有需付费 larger runner

### 验证
1. 本地：`cargo check`（确认 Windows 零破坏）+ `npm run build`
2. Actions 页手动 Run workflow → 4 job 全绿、artifacts 齐全
3. 打 tag v0.0.3 → Release 自动含 dmg/deb/rpm
4. 真机安装测试：mac dmg 双击；linux `dpkg -i` / `rpm -i`（验证 pdfium 库随资源包正确加载）

### 预计耗时
- mac job ~25 分钟（双架构无缓存，缓存后 ~12 分钟）
- linux 两个 job 各 ~8 分钟，与 mac 并行
- 总耗时 ~25 分钟（受最慢 job 限制）

---

## 视频转 PDF（教材化）— 纯 Rust 方案 ✅ 已实施

### 功能概述
将本地视频/音频转成带截图的 PDF 教材：提取音频 → ASR 转写（自动切片）→ 机械分章 → 场景取帧 → OCR 配图说明 → genpdf 排版生成图文 PDF。**全 Rust 实现，零 Python 依赖**。

### 已确认决策（已落地）
- **ASR 引擎**：默认 **Fun-ASR-Nano**（sherpa-onnx 官方 crate，LLM 级中文识别），Paraformer 可选；**UI 下拉框切换**
- **运行方式**：纯 CPU（RTX 3060 Ti 实测 ~1.5x 实时；官方 RTF 0.16）
- **工具链**：MSVC（已装 VS Build Tools 17.14 via winget 直连，rustup 切 stable-msvc）
- **ffmpeg**：随包捆绑（开发时 msys2 mingw64）
- **模型源**：默认 **ModelScope**（国内直连），GitHub 回退
- **UI**：独立「视频转PDF」标签页 + 引擎下拉 + 分阶段进度 + 取消
- **集成方式**：纯 Rust（sherpa-onnx 官方 crate 静态链接）

### 关键实现（src-tauri/src/v2p/）
- `asr.rs`：sherpa-onnx 封装；**Fun-ASR-Nano 12s/段自动切片**（max_total_len 512 限制，0.5s 重叠防切词）+ 句级时间戳 + 进度/取消回调
- `ffmpeg.rs`：音频提取（16kHz mono wav）+ 场景取帧 + 关键帧
- `ocr.rs`（win）：图片文件 OCR（复用 ocr.rs WinRT）
- `chapters.rs`：机械分章（时间等分聚合 segments）
- `pdf.rs`：genpdf 排版（封面/章节/图文，中文字体）
- `commands.rs`：`v2p_check_env` / `v2p_transcribe` 命令，事件流 `v2p:progress` / `v2p:done`，SearchState 取消

### 已修复的底层 bug
- **ocr.rs**：`Buffer` cast 到 `IMemoryBufferByteAccess` 错误 → 改用 `IBufferByteAccess`——**修复了 PDFind 一直存在的 OCR 静默失败问题**

### 依赖（Cargo.toml）
```toml
sherpa-onnx = "1.13"
image = { version = "0.25", default-features = false, features = ["jpeg","png"] }
genpdf = { version = "0.2", features = ["images"] }
```

### 测试通过
- `v2p::asr`：FunASR 单段转写 ✓ / 45s 长音频切片转写（1252 字符/13 段）✓
- `v2p::ocr`：真实帧图片 OCR（识别出"上证指数 4027.26"等）✓
- `v2p::pdf`：真实 PDF 生成（中文正常嵌入）✓
- `v2p::chapters`：分章逻辑 ✓

### 模型资源
- Fun-ASR-Nano int8: `sherpa-onnx-funasr-nano-int8-2025-12-30`（~950MB, encoder+llm+embedding+Qwen3-0.6B tokenizer）
- Paraformer: `sherpa-onnx-paraformer-zh-2024-03-09`（~230MB, 待下载）
- 开发缓存：`src-tauri/dev-models/`（gitignore）
- 打包后：模型由用户下载至配置目录（后续实现自动下载）

### 剩余 TODO
- [ ] 模型自动下载（打包后首次使用从 ModelScope 拉取，带进度）
- [ ] 取帧+OCR+分章整合进完整 `video_to_pdf` 命令（目前转写命令已通）
- [ ] 输出路径选择 + 生成 PDF 的完整 UI 流程
- [ ] 正式捆绑 ffmpeg.exe + Noto Sans SC 字体到发布包
- [ ] LLM 智能分章（预留接口）

---

## 其他待办（如有）
（暂无）
