# PDFind

PDF 全文搜索桌面工具——在单个 PDF 文件或整个目录（含子目录）中快速搜索关键词，命中结果点击即可在预览面板中定位到对应页面并高亮显示。

## 功能特性

- **双模式搜索**
  - 单文件搜索：即时返回全部命中
  - 目录搜索：递归扫描子目录，流式推送结果，不阻塞界面
- **匹配选项**：忽略大小写、整词匹配（前后端使用一致的正则规则）
- **内置预览**：基于 pdf.js 的渲染面板，懒加载 + 视口触发渲染，高分屏清晰显示
- **关键词高亮**：预览页面中按文本层坐标精确高亮命中串
- **进度与取消**：实时进度条（已扫描/命中数/当前文件），随时取消任务
- **可拖拽分栏**：搜索结果与预览面板宽度可自由调整

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | [Tauri 2](https://tauri.app/) |
| 前端 | Vue 3 + TypeScript + Vite |
| 文本提取（后端） | [pdfium-render](https://crates.io/crates/pdfium-render)（绑定 pdfium 动态库） |
| 预览渲染（前端） | [pdfjs-dist](https://www.npmjs.com/package/pdfjs-dist) |
| 目录遍历 | [ignore](https://crates.io/crates/ignore)（尊重 .gitignore） |
| 匹配引擎 | Rust [regex](https://crates.io/crates/regex) |

## 环境准备

1. [Node.js](https://nodejs.org/) ≥ 18
2. [Rust](https://www.rust-lang.org/) 工具链（`rustup` 默认 stable 即可）
3. [Tauri 2 前置依赖](https://tauri.app/start/prerequisites/)（Windows 需 Visual Studio C++ 构建工具）
4. **pdfium 动态库**（不入库，需手动放置）：

   从 [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases) 下载对应平台版本，
   将 `pdfium.dll` 放到 `src-tauri/binaries/` 目录下。

   > 目前仅适配 Windows；跨平台需自行补充对应平台的动态库文件。

## 快速开始

```bash
# 安装前端依赖
npm install

# 开发模式运行
npm run tauri dev

# 打包发布（产物在 src-tauri/target/release/bundle/）
npm run tauri build
```

## 项目结构

```
├── src/                        # 前端
│   ├── App.vue                 # 主界面：搜索工具栏、进度、结果列表
│   └── components/
│       └── Preview.vue         # PDF 预览面板（渲染 + 高亮 + 页码导航）
└── src-tauri/                  # 后端
    ├── binaries/               # pdfium.dll（gitignore，手动放置）
    └── src/
        ├── lib.rs              # Tauri 入口，注册命令与状态
        ├── commands.rs         # 命令与事件：单文件搜索 / 目录搜索 / 取消
        ├── engine.rs           # 正则匹配器（转义、整词、大小写）
        ├── pdfx.rs             # pdfium 加载与逐页文本提取
        ├── walker.rs           # 目录递归收集 PDF（支持取消）
        └── state.rs            # 搜索任务状态（单任务护栏 + 取消标志）
```

## 使用说明

1. 选择单个 PDF 文件或一个目录（目录模式自动递归子目录）
2. 输入关键词，按需勾选「忽略大小写 / 整词匹配」
3. 点击「搜索」或回车开始；目录搜索过程中可随时「取消」
4. 点击命中行 → 右侧打开预览并跳转到对应页面，关键词自动高亮
5. 拖动中间分隔条可调整预览宽度，双击分隔条恢复默认
