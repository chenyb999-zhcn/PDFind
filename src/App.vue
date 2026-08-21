<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Preview from "./components/Preview.vue";

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

// 预览面板状态
const previewPath = ref("");
const previewPage = ref(1);
const previewW = ref<number | null>(null); // 拖拽后的像素宽, null=默认 46%

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
    const w = startW + (startX - ev.clientX);
    const max = window.innerWidth - 480; // 左侧搜索区保底宽度
    previewW.value = Math.round(Math.min(Math.max(w, 280), Math.max(280, max)));
  };
  const onUp = () => {
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
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
    }),
  );
  unlisteners.push(
    await listen<string>("search:error", (e) => {
      error.value = e.payload;
      searching.value = false;
      prog.value = null;
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
  }
}

async function browseDir() {
  const sel = await open({ multiple: false, directory: true });
  if (typeof sel === "string") {
    filePath.value = sel;
    isDir.value = true;
  }
}

async function search() {
  error.value = "";
  results.value = [];
  done.value = null;
  prog.value = null;
  if (!filePath.value || !keyword.value) {
    error.value = "请先选择文件/目录并输入搜索词";
    return;
  }
  searching.value = true;
  try {
    if (isDir.value) {
      await invoke("start_search", {
        path: filePath.value,
        pattern: keyword.value,
        caseInsensitive: caseInsensitive.value,
        wholeWord: wholeWord.value,
      });
      // 结果经事件流到达, searching 在 search:done 复位
    } else {
      const r = await invoke<FileSearchResult>("search_file", {
        path: filePath.value,
        pattern: keyword.value,
        caseInsensitive: caseInsensitive.value,
        wholeWord: wholeWord.value,
      });
      results.value = [r];
      searching.value = false;
    }
  } catch (e) {
    error.value = String(e);
    searching.value = false;
  }
}

async function cancel() {
  try {
    await invoke("cancel_search");
  } catch (e) {
    error.value = String(e);
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

const totalHits = () => results.value.reduce((s, r) => s + r.hits.length, 0);
</script>

<template>
  <main class="wrap">
    <div class="body">
      <div class="left">
        <div class="toolbar">
          <input
            class="path"
            v-model="filePath"
            :placeholder="isDir ? '目录路径…' : 'PDF 文件路径…'"
            @keyup.enter="search"
          />
          <button @click="browseFile">选文件…</button>
          <button @click="browseDir">选目录…</button>
          <input
            class="kw"
            v-model="keyword"
            placeholder="搜索词"
            @keyup.enter="search"
          />
          <button v-if="!searching" class="primary" @click="search">
            搜索
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
        @dblclick="previewW = null"
      ></div>

      <Preview
        v-if="previewPath"
        :key="previewPath"
        class="preview-panel"
        :style="previewW != null ? { flex: `0 0 ${previewW}px` } : undefined"
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
}
.body {
  display: flex;
  gap: 0;
  height: 100%;
  min-height: 0;
}
.left {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
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
  flex: 0 0 46%;
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
</style>
