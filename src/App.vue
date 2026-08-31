<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Preview from "./components/Preview.vue";
import DirTree from "./components/DirTree.vue";
import VideoToPdf from "./components/VideoToPdf.vue";
import KnowledgeBase from "./components/KnowledgeBase.vue";

interface Hit {
  page: number;
  pre: string;
  matched: string;
  post: string;
}
interface FileSearchResult {
  path: string;
  total_pages: number;
  hits: Hit[];
}
interface ProgressEvent {
  scanned: number;
  total: number;
  matched: number;
  current: string;
}
interface DoneEvent {
  cancelled: boolean;
  scanned: number;
  total: number;
  matched: number;
  skipped: number;
  hits: number;
}

const filePath = ref("");
const keyword = ref("");
const isDir = ref(false);
const caseInsensitive = ref(true);
const wholeWord = ref(false);
const searching = ref(false);
const error = ref("");
const results = ref<FileSearchResult[]>([]);
const prog = ref<ProgressEvent | null>(null);
const done = ref<DoneEvent | null>(null);

// 预览面板状态: 宽度按"比例"持久化, 窗口缩放时保持占比; null=默认 50%
const previewPath = ref("");
const previewPage = ref(1);
const PREVIEW_R_KEY = "preview.ratio";
const previewRatio = ref<number | null>(null);

function bodyWidth(): number {
  return (
    document.querySelector<HTMLElement>(".body")?.clientWidth ||
    window.innerWidth
  );
}

// 像素钳制: 预览至少 280px, 且给左侧搜索区保底约 480px
function clampPreviewPx(w: number): number {
  const bw = bodyWidth();
  return Math.round(Math.min(Math.max(w, 280), Math.max(280, bw - 480)));
}

function loadPreviewW() {
  try {
    const saved = Number(localStorage.getItem(PREVIEW_R_KEY));
    if (Number.isFinite(saved) && saved > 0.05 && saved < 1) {
      previewRatio.value = saved;
    }
    localStorage.removeItem("preview.width"); // 清理旧版像素记录
  } catch {
    /* ignore */
  }
}

function resetPreviewW() {
  previewRatio.value = null;
  try {
    localStorage.removeItem(PREVIEW_R_KEY);
  } catch {
    /* ignore */
  }
}

// 目录树面板状态(显隐持久化)
const treeVisible = ref(
  (() => {
    try {
      return localStorage.getItem("dirtree.visible") !== "0";
    } catch {
      return true;
    }
  })(),
);

// 顶部标签页: "search" | "v2p" | "kb"
const activeTab = ref<"search" | "v2p" | "kb">("search");

// 搜索关键字历史记录 (最多10条，本地持久化)
const keywordHistory = ref<string[]>([]);
onMounted(() => {
  try {
    const saved = localStorage.getItem("search.history");
    keywordHistory.value = saved ? JSON.parse(saved) : [];
  } catch {
    keywordHistory.value = [];
  }
  loadPreviewW();
});
const showHistory = ref(false);
const historyFocused = ref(false);
const kwWrapRef = ref<HTMLDivElement | null>(null);

// 加载状态：搜索开始2秒后显示
const showLoading = ref(false);
const searchStartTime = ref<number | null>(null);
const loadingTimer = ref<number | null>(null);
const ocrMode = ref(false); // 当前搜索是否为 OCR 模式(遮罩文案用)

function toggleTree() {
  treeVisible.value = !treeVisible.value;
  try {
    localStorage.setItem("dirtree.visible", treeVisible.value ? "1" : "0");
  } catch {
    /* ignore */
  }
}

// 目录树点击: 只填路径, 不自动搜索
function onTreePick(path: string, dirMode: boolean) {
  filePath.value = path;
  isDir.value = dirMode;
}

// 分隔条拖拽: 预览在右侧, 向左拖增宽
function startDrag(e: MouseEvent) {
  const target = document.querySelector<HTMLElement>(".preview-panel");
  if (!target) return;
  e.preventDefault();
  const startX = e.clientX;
  const startW = target.getBoundingClientRect().width;
  document.body.style.userSelect = "none";
  document.body.style.cursor = "col-resize";
  const onMove = (ev: MouseEvent) => {
    const w = clampPreviewPx(startW + (startX - ev.clientX));
    previewRatio.value = w / bodyWidth();
  };
  const onUp = () => {
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
    // 记住用户调整的宽度比例(窗口缩放后仍保持该占比)
    try {
      if (previewRatio.value != null) {
        localStorage.setItem(PREVIEW_R_KEY, String(previewRatio.value));
      }
    } catch {
      /* ignore */
    }
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

const unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  unlisteners.push(
    await listen<ProgressEvent>("search:progress", (e) => {
      prog.value = e.payload;
    }),
  );
  unlisteners.push(
    await listen<FileSearchResult>("search:result", (e) => {
      results.value.push(e.payload);
    }),
  );
  unlisteners.push(
    await listen<DoneEvent>("search:done", (e) => {
      done.value = e.payload;
      searching.value = false;
      prog.value = null;
      clearLoadingTimer();
    }),
  );
  unlisteners.push(
    await listen<string>("search:error", (e) => {
      error.value = e.payload;
      searching.value = false;
      prog.value = null;
      clearLoadingTimer();
    }),
  );
});

onUnmounted(() => {
  unlisteners.forEach((u) => u());
});

async function browseFile() {
  const sel = await open({
    multiple: false,
    filters: [{ name: "PDF 文件", extensions: ["pdf"] }],
  });
  if (typeof sel === "string") {
    filePath.value = sel;
    isDir.value = false;
    // 选中文件后自动打开预览，显示第一页
    openPreview(sel, 1);
  }
}

async function browseDir() {
  const sel = await open({ multiple: false, directory: true });
  if (typeof sel === "string") {
    filePath.value = sel;
    isDir.value = true;
  }
}

async function search(useOcr = false) {
  error.value = "";
  results.value = [];
  done.value = null;
  prog.value = null;
  if (!filePath.value || !keyword.value) {
    error.value = "请先选择文件/目录并输入搜索词";
    return;
  }
  addToHistory(keyword.value);
  searching.value = true;
  ocrMode.value = useOcr;
  startLoadingTimer();
  try {
    const args = {
      path: filePath.value,
      pattern: keyword.value,
      caseInsensitive: caseInsensitive.value,
      wholeWord: wholeWord.value,
      useOcr,
    };
    if (isDir.value) {
      await invoke("start_search", args);
    } else {
      const r = await invoke<FileSearchResult>("search_file", args);
      results.value = [r];
      searching.value = false;
    }
  } catch (e) {
    // 用户主动取消: 静默复位(含按钮), 不作为错误提示
    if (!String(e).includes("已取消")) {
      error.value = String(e);
    }
    searching.value = false;
  } finally {
    clearLoadingTimer();
  }
}

async function cancel() {
  try {
    await invoke("cancel_search");
  } catch (e) {
    error.value = String(e);
  } finally {
    clearLoadingTimer();
  }
}

// 点击命中行: 右侧打开预览并跳到对应页
function openPreview(path: string, page: number) {
  previewPath.value = path;
  previewPage.value = page;
}

function basename(p: string): string {
  return p.split(/[\\/]/).pop() || p;
}

// 搜索关键字历史相关
function addToHistory(kw: string) {
  const trimmed = kw.trim();
  if (!trimmed) return;
  const idx = keywordHistory.value.indexOf(trimmed);
  if (idx >= 0) keywordHistory.value.splice(idx, 1);
  keywordHistory.value.unshift(trimmed);
  if (keywordHistory.value.length > 10) keywordHistory.value.length = 10;
  try {
    localStorage.setItem("search.history", JSON.stringify(keywordHistory.value));
  } catch {
    /* ignore */
  }
}

function selectHistoryItem(kw: string) {
  keyword.value = kw;
  showHistory.value = false;
  historyFocused.value = false;
  search();
}

function onKeywordFocus() {
  historyFocused.value = true;
  if (keywordHistory.value.length > 0) showHistory.value = true;
}

function onKeywordBlur() {
  setTimeout(() => {
    showHistory.value = false;
    historyFocused.value = false;
  }, 150);
}

function onHistoryMouseDown(e: MouseEvent) {
  e.preventDefault(); // 防止 blur 先于 click 触发
}

// 加载状态控制
function startLoadingTimer() {
  searchStartTime.value = Date.now();
  showLoading.value = false;
  loadingTimer.value = window.setTimeout(() => {
    if (searching.value) showLoading.value = true;
  }, 2000);
}

function clearLoadingTimer() {
  if (loadingTimer.value) {
    clearTimeout(loadingTimer.value);
    loadingTimer.value = null;
  }
  showLoading.value = false;
  searchStartTime.value = null;
}

const totalHits = () => results.value.reduce((s, r) => s + r.hits.length, 0);
</script>

<template>
  <main class="wrap">
    <div class="tabs">
      <button
        class="tab"
        :class="{ on: activeTab === 'search' }"
        @click="activeTab = 'search'"
      >
        搜索
      </button>
      <button
        class="tab"
        :class="{ on: activeTab === 'v2p' }"
        @click="activeTab = 'v2p'"
      >
        视频转 PDF
      </button>
      <button
        class="tab"
        :class="{ on: activeTab === 'kb' }"
        @click="activeTab = 'kb'"
      >
        知识库
      </button>
    </div>
    <div v-if="activeTab === 'v2p'" class="v2p-body">
      <VideoToPdf />
    </div>
    <div v-else-if="activeTab === 'kb'" class="v2p-body">
      <KnowledgeBase />
    </div>
    <div v-else class="body">
      <DirTree v-show="treeVisible" class="tree-panel" @pick="onTreePick" />
      <div
        class="tree-toggle"
        :title="treeVisible ? '隐藏目录树' : '显示目录树'"
        @click="toggleTree"
      >
        {{ treeVisible ? "‹" : "›" }}
      </div>
      <div class="left">
        <div class="toolbar">
          <input
            class="path"
            v-model="filePath"
            :placeholder="isDir ? '目录路径…' : 'PDF 文件路径…'"
            @keyup.enter="search(false)"
          />
          <button @click="browseFile">选文件…</button>
          <button @click="browseDir">选目录…</button>
          <div class="kw-wrap" ref="kwWrapRef">
            <input
              class="kw"
              v-model="keyword"
              placeholder="搜索词"
              @keyup.enter="search(false)"
              @focus="onKeywordFocus"
              @blur="onKeywordBlur"
            />
            <div
              v-show="showHistory && keywordHistory.length > 0"
              class="kw-history"
              @mousedown="onHistoryMouseDown"
            >
              <div
                v-for="(kw, i) in keywordHistory"
                :key="i"
                class="kw-history-item"
                @click="selectHistoryItem(kw)"
              >
                {{ kw }}
              </div>
            </div>
          </div>
          <button v-if="!searching" class="primary" @click="search(false)">
            搜索
          </button>
          <button v-if="!searching" class="ocr" @click="search(true)">
            OCR搜索
          </button>
          <button v-else class="danger" @click="cancel">取消</button>
        </div>

        <div class="opts">
          <label
            ><input type="checkbox" v-model="caseInsensitive" /> 忽略大小写</label
          >
          <label><input type="checkbox" v-model="wholeWord" /> 整词匹配</label>
          <span v-if="isDir" class="mode-tag">目录模式(含子目录)</span>
        </div>

        <p v-if="error" class="error">{{ error }}</p>

        <div v-if="prog" class="progress">
          <div class="bar">
            <div
              class="fill"
              :style="{
                width:
                  prog.total ? (prog.scanned / prog.total) * 100 + '%' : '0%',
              }"
            />
          </div>
          <span class="ptext"
            >{{ prog.scanned }}/{{ prog.total }} · 命中 {{ prog.matched }} 个文件
            · {{ prog.current }}</span
          >
        </div>

        <p v-if="done" class="summary">
          {{ done.cancelled ? "已取消 · " : "完成 · " }}扫描 {{ done.scanned }}/{{
            done.total
          }} · 命中 {{ done.matched }} 个文件 {{ done.hits }} 行<template
            v-if="done.skipped"
            >· 跳过(加密/损坏) {{ done.skipped }}</template
          >
        </p>

        <section class="result">
          <div v-show="showLoading" class="loading-overlay">
            <div class="loading-spinner"></div>
            <span>{{ ocrMode ? "正在搜索(含OCR)..." : "正在搜索..." }}</span>
          </div>
          <header v-if="results.length" class="result-head">
            <span>命中 {{ results.length }} 个文件 · {{ totalHits() }} 行</span>
          </header>
          <p
            v-if="done && !done.cancelled && results.length === 0"
            class="empty"
          >
            没有找到匹配内容
          </p>
          <div class="groups">
            <details
              v-for="r in results"
              :key="r.path"
              :open="!isDir || results.length <= 3"
              class="group"
            >
              <summary>
                <span class="gfile">{{ basename(r.path) }}</span>
                <span class="gstat"
                  >{{ r.hits.length }} 行 / {{ r.total_pages }} 页</span
                >
                <span class="gpath">{{ r.path }}</span>
              </summary>
              <ul class="hits">
                <li
                  v-for="(h, i) in r.hits"
                  :key="i"
                  :class="{
                    active:
                      previewPath === r.path && previewPage === h.page,
                  }"
                  @click="openPreview(r.path, h.page)"
                >
                  <span class="pg">第 {{ h.page }} 页</span
                  ><span class="line"
                    >{{ h.pre }}<mark>{{ h.matched }}</mark>{{ h.post }}</span
                  >
                </li>
              </ul>
            </details>
          </div>
        </section>
      </div>

      <div
        v-if="previewPath"
        class="splitter"
        title="拖动调整宽度"
        @mousedown="startDrag"
        @dblclick="resetPreviewW"
      ></div>

      <Preview
        v-if="previewPath"
        :key="previewPath"
        class="preview-panel"
        :style="
          previewRatio != null
            ? { flexBasis: previewRatio * 100 + '%' }
            : undefined
        "
        :path="previewPath"
        :page="previewPage"
        :keyword="keyword"
        :case-insensitive="caseInsensitive"
        :whole-word="wholeWord"
        @close="previewPath = ''"
      />
    </div>
  </main>
</template>

<style>
:root {
  font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
  font-size: 14px;
  color: #1f2328;
  background-color: #f6f8fa;
}
* {
  box-sizing: border-box;
}
body {
  margin: 0;
}
</style>

<style scoped>
.wrap {
  padding: 16px;
  height: 100vh;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.tabs {
  display: flex;
  gap: 4px;
  border-bottom: 1px solid #d0d7de;
  padding-bottom: 6px;
}
.tab {
  padding: 5px 16px;
  border: 1px solid transparent;
  border-radius: 6px 6px 0 0;
  background: transparent;
  cursor: pointer;
  color: #57606a;
}
.tab.on {
  background: #fff;
  border-color: #d0d7de;
  border-bottom-color: #fff;
  color: #1f2328;
  font-weight: 600;
}
.v2p-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: #fff;
  border: 1px solid #d0d7de;
  border-radius: 8px;
}
.body {
  display: flex;
  gap: 0;
  flex: 1;
  min-height: 0;
}
.left {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.tree-panel {
  flex: 0 0 240px;
}
.tree-toggle {
  flex: 0 0 16px;
  margin: 0 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  color: #57606a;
  cursor: pointer;
  user-select: none;
  font-size: 14px;
}
.tree-toggle:hover {
  background: #eaeef2;
  color: #1f6feb;
}
.splitter {
  flex: 0 0 9px;
  margin: 0 4px;
  cursor: col-resize;
  border-radius: 4px;
  background: transparent;
  position: relative;
}
.splitter::after {
  content: "";
  position: absolute;
  left: 3px;
  top: 0;
  bottom: 0;
  width: 3px;
  border-radius: 2px;
  background: #d0d7de;
  transition: background 0.15s;
}
.splitter:hover::after {
  background: #1f6feb;
}
.preview-panel {
  flex: 0 0 50%;
  min-width: 280px;
  max-width: calc(100% - 480px);
}
.toolbar {
  display: flex;
  gap: 8px;
}
.toolbar input {
  padding: 6px 10px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
}
.toolbar .path {
  flex: 3;
  min-width: 120px;
}
.toolbar .kw {
  flex: 1;
  min-width: 90px;
}
.toolbar button {
  padding: 6px 14px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
  white-space: nowrap;
}
.toolbar button.primary {
  background: #1f6feb;
  border-color: #1f6feb;
  color: #fff;
}
.toolbar button.ocr {
  background: #1a7f37;
  border-color: #1a7f37;
  color: #fff;
}
.toolbar button.danger {
  background: #cf222e;
  border-color: #cf222e;
  color: #fff;
}
.toolbar button:disabled {
  opacity: 0.6;
  cursor: default;
}
.opts {
  display: flex;
  gap: 18px;
  align-items: center;
  color: #57606a;
}
.mode-tag {
  color: #1f6feb;
}
.error {
  color: #cf222e;
  margin: 0;
}
.progress {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.bar {
  height: 6px;
  border-radius: 3px;
  background: #d0d7de;
  overflow: hidden;
}
.fill {
  height: 100%;
  background: #1f6feb;
  transition: width 0.15s;
}
.ptext {
  color: #57606a;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.summary {
  margin: 0;
  color: #1a7f37;
}
.result {
  flex: 1;
  min-height: 0;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  background: #fff;
  display: flex;
  flex-direction: column;
  position: relative;
}
.result-head {
  padding: 8px 12px;
  border-bottom: 1px solid #d0d7de;
  background: #f6f8fa;
  border-radius: 8px 8px 0 0;
  color: #57606a;
}
.empty {
  padding: 24px;
  text-align: center;
  color: #57606a;
}
.groups {
  overflow: auto;
  flex: 1;
  padding: 4px 0;
}
.group {
  border-bottom: 1px solid #eaeef2;
}
.group summary {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 6px 12px;
  cursor: pointer;
  user-select: none;
}
.group summary:hover {
  background: #f3f4f6;
}
.gfile {
  font-weight: 600;
  white-space: nowrap;
}
.gstat {
  color: #1f6feb;
  white-space: nowrap;
}
.gpath {
  color: #8b949e;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}
.hits {
  list-style: none;
  margin: 0;
  padding: 2px 0 6px;
}
.hits li {
  display: flex;
  gap: 10px;
  padding: 3px 12px 3px 28px;
  line-height: 1.6;
  cursor: pointer;
}
.hits li:hover {
  background: #f0f6ff;
}
.hits li.active {
  background: #dbeafe;
}
.hits .pg {
  flex: none;
  color: #1f6feb;
  min-width: 64px;
}
.hits .line {
  user-select: text;
  overflow-wrap: anywhere;
}
.hits mark {
  background: #fff3c4;
  padding: 0 1px;
  border-radius: 2px;
}

/* 关键字历史下拉 */
.kw-wrap {
  position: relative;
  flex: 1;
  min-width: 90px;
}
.kw-wrap input {
  width: 100%;
}
.kw-history {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  margin-top: 4px;
  background: #fff;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.1);
  z-index: 100;
  max-height: 200px;
  overflow-y: auto;
}
.kw-history-item {
  padding: 6px 10px;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.kw-history-item:hover {
  background: #f0f6ff;
}

/* 加载遮罩 */
.loading-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: rgba(255,255,255,0.9);
  z-index: 10;
  border-radius: 8px;
}
.loading-spinner {
  width: 24px;
  height: 24px;
  border: 3px solid #d0d7de;
  border-top-color: #1f6feb;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>