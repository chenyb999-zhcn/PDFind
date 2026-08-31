<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface KbDoc {
  id: number;
  kind: string;
  title: string;
  source_path: string;
  pdf_path: string | null;
  lang: string;
  duration_s: number;
  n_chunks: number;
  created_at: number;
}
interface KbOverview {
  docs: KbDoc[];
  chunks: number;
  embed_model: boolean;
}
interface AskSource {
  doc_id: number;
  doc_title: string;
  chapter: string;
  start_s: number;
  end_s: number;
  page: number;
  snippet: string;
}
interface AskResult {
  answer: string;
  sources: AskSource[];
}
interface Hit {
  chunk_id: number;
  doc_id: number;
  doc_title: string;
  chapter: string;
  text: string;
  start_s: number;
  end_s: number;
  page: number;
  score: number;
}
interface OrganizerInfo {
  id: string;
  name: string;
  has_key: boolean;
}
interface OrganizerConfig {
  keys: Record<string, string>;
  models: Record<string, string>;
  custom: { base_url: string; model: string; api_key: string };
}
interface DlProg {
  done: number;
  total: number;
}
interface Msg {
  role: "user" | "assistant";
  text: string;
  sources?: AskSource[];
}

const overview = ref<KbOverview | null>(null);
const providers = ref<OrganizerInfo[]>([]);
const orgCfg = ref<OrganizerConfig>({
  keys: {},
  models: {},
  custom: { base_url: "", model: "", api_key: "" },
});
const providerId = ref("");
const question = ref("");
const asking = ref(false);
const searching = ref(false);
const error = ref("");
const logs = ref<string[]>([]);
const dlMsg = ref("");
const downloadingModel = ref(false);
const addingDoc = ref(false);
const msgs = ref<Msg[]>([]);
const searchHits = ref<Hit[] | null>(null);

const unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  unlisteners.push(
    await listen<string>("kb:log", (e) => {
      logs.value.push(e.payload);
      if (logs.value.length > 500) logs.value.shift();
    }),
  );
  unlisteners.push(
    await listen<DlProg>("kb:dl", (e) => {
      const p = e.payload;
      dlMsg.value = `下载中: ${fmtBytes(p.done)}/${fmtBytes(p.total)}`;
    }),
  );
  try {
    const env = await invoke<{ organizers: OrganizerInfo[] }>("v2p_check_env");
    providers.value = env.organizers;
    orgCfg.value = await invoke<OrganizerConfig>("v2p_get_organizer_config");
    // 默认选第一个已配 Key 的服务商
    const withKey = providers.value.find((p) => hasKey(p));
    providerId.value = (withKey || providers.value[0])?.id || "";
  } catch (e) {
    console.error("加载服务商配置失败", e);
  }
  await refresh();
});

onUnmounted(() => {
  unlisteners.forEach((u) => u());
});

function fmtBytes(b: number): string {
  if (b > 1024 * 1024) return (b / 1024 / 1024).toFixed(1) + " MB";
  return Math.ceil(b / 1024) + " KB";
}

function fmtTs(s: number): string {
  if (!isFinite(s) || s <= 0) return "";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

function fmtDate(ts: number): string {
  const d = new Date(ts * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function hasKey(p: OrganizerInfo): boolean {
  if (p.id === "custom") return !!orgCfg.value.custom.api_key;
  return !!orgCfg.value.keys[p.id];
}

function locOf(s: AskSource): string {
  if (s.page > 0) return `第${s.page}页`;
  if (s.end_s > 0) return `${fmtTs(s.start_s)}-${fmtTs(s.end_s)}`;
  return "";
}

function kindIcon(kind: string): string {
  if (kind === "video") return "🎬";
  if (kind === "pdf") return "📄";
  return "📝";
}

async function refresh() {
  try {
    overview.value = await invoke<KbOverview>("kb_overview");
  } catch (e) {
    error.value = String(e);
  }
}

async function addPdf() {
  const sel = await open({
    multiple: false,
    filters: [{ name: "PDF 文件", extensions: ["pdf"] }],
  });
  if (typeof sel !== "string") return;
  await ingest(sel, "kb_add_pdf");
}

async function addText() {
  const sel = await open({
    multiple: false,
    filters: [{ name: "文本文件", extensions: ["txt", "md"] }],
  });
  if (typeof sel !== "string") return;
  await ingest(sel, "kb_add_text");
}

async function ingest(path: string, cmd: string) {
  error.value = "";
  addingDoc.value = true;
  dlMsg.value = "入库中(分块+编码)…";
  try {
    const r = await invoke<{ chunks: number; embedded: boolean }>(cmd, {
      path,
    });
    dlMsg.value = `入库完成: ${r.chunks} 块${r.embedded ? "" : "(无向量, 模型未下载)"}`;
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    addingDoc.value = false;
  }
}

async function removeDoc(id: number) {
  try {
    await invoke("kb_remove_doc", { id });
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

async function downloadEmbedModel() {
  error.value = "";
  downloadingModel.value = true;
  dlMsg.value = "";
  try {
    await invoke("kb_download_embed_model");
    dlMsg.value = "模型下载完成";
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    downloadingModel.value = false;
  }
}

async function ask() {
  const q = question.value.trim();
  if (!q || asking.value) return;
  if (!providerId.value) {
    error.value = "请先选择问答服务商";
    return;
  }
  error.value = "";
  searchHits.value = null;
  msgs.value.push({ role: "user", text: q });
  question.value = "";
  asking.value = true;
  try {
    const r = await invoke<AskResult>("kb_ask", {
      question: q,
      providerId: providerId.value,
    });
    msgs.value.push({ role: "assistant", text: r.answer, sources: r.sources });
  } catch (e) {
    msgs.value.push({ role: "assistant", text: `⚠️ ${String(e)}` });
  } finally {
    asking.value = false;
  }
}

async function searchNow() {
  const q = question.value.trim();
  if (!q || searching.value) return;
  error.value = "";
  searching.value = true;
  try {
    searchHits.value = await invoke<Hit[]>("kb_search", { question: q });
  } catch (e) {
    error.value = String(e);
  } finally {
    searching.value = false;
  }
}
</script>

<template>
  <div class="kb">
    <div class="kb-head">
      <h2>知识库</h2>
      <span v-if="overview" class="stat">
        {{ overview.docs.length }} 文档 · {{ overview.chunks }} 块 ·
        {{ overview.embed_model ? "向量+关键词检索" : "仅关键词检索" }}
      </span>
    </div>

    <div v-if="overview && !overview.embed_model" class="model-card">
      <span>
        向量语义检索需下载 embedding 模型 (BGE-small-zh, 约 26MB, 本地离线)
      </span>
      <button :disabled="downloadingModel" @click="downloadEmbedModel">
        {{ downloadingModel ? dlMsg || "下载中…" : "下载模型" }}
      </button>
    </div>

    <div class="kb-main">
      <div class="docs">
        <div class="docs-head">
          <button :disabled="addingDoc" @click="addPdf">+ PDF</button>
          <button :disabled="addingDoc" @click="addText">+ 文本</button>
        </div>
        <div class="doc-list">
          <div v-for="d in overview?.docs" :key="d.id" class="doc">
            <div class="doc-title">
              {{ kindIcon(d.kind) }} {{ d.title }}
            </div>
            <div class="doc-meta">
              {{ d.n_chunks }} 块 · {{ fmtDate(d.created_at) }}
            </div>
            <div class="doc-actions">
              <button class="mini" @click="removeDoc(d.id)">删除</button>
            </div>
          </div>
          <p v-if="overview && !overview.docs.length" class="empty">
            暂无文档。在「视频转 PDF」生成时勾选「存入知识库」，或用上方按钮添加
            PDF / 文本。
          </p>
        </div>
        <p v-if="dlMsg" class="ok">{{ dlMsg }}</p>
      </div>

      <div class="chat">
        <div class="chat-msgs">
          <div v-for="(m, i) in msgs" :key="i" :class="['msg', m.role]">
            <div class="msg-text">{{ m.text }}</div>
            <div v-if="m.sources && m.sources.length" class="srcs">
              <div v-for="(s, j) in m.sources" :key="j" class="src">
                <span class="src-tag">[{{ j + 1 }}]</span
                >《{{ s.doc_title }}》
                <span v-if="locOf(s)" class="src-loc">{{ locOf(s) }}</span>
                <span class="src-snip">{{ s.snippet }}</span>
              </div>
            </div>
          </div>

          <div v-if="searchHits" class="hits">
            <p class="hits-title">检索结果 ({{ searchHits.length }})</p>
            <div v-if="!searchHits.length" class="empty">没有匹配内容</div>
            <div v-for="h in searchHits" :key="h.chunk_id" class="hit">
              <span class="hit-where">
                《{{ h.doc_title }}》
                <template v-if="h.page > 0">第{{ h.page }}页</template>
                <template v-else-if="h.end_s > 0">{{ fmtTs(h.start_s) }}-{{ fmtTs(h.end_s) }}</template>
              </span>
              <span class="hit-text">{{ h.text }}</span>
            </div>
          </div>

          <p
            v-if="!msgs.length && !searchHits"
            class="empty"
          >
            输入问题开始问答（需先配置 API Key，同「视频转 PDF」页）；或点「检索」只做关键词/向量搜索。
          </p>
        </div>

        <div class="chat-input">
          <select v-model="providerId" title="问答服务商">
            <option v-for="p in providers" :key="p.id" :value="p.id">
              {{ p.name }} {{ hasKey(p) ? "✓" : "(未配Key)" }}
            </option>
          </select>
          <input
            class="q"
            v-model="question"
            placeholder="向知识库提问…（Enter 提问）"
            @keyup.enter="ask"
          />
          <button
            class="primary"
            :disabled="asking || !question.trim()"
            @click="ask"
          >
            {{ asking ? "回答中…" : "提问" }}
          </button>
          <button
            :disabled="searching || !question.trim()"
            @click="searchNow"
          >
            {{ searching ? "检索中…" : "检索" }}
          </button>
        </div>
      </div>
    </div>

    <p v-if="error" class="error">{{ error }}</p>

    <div v-if="logs.length" class="log-panel">
      <div class="log-head">日志 ({{ logs.length }})</div>
      <div class="log-body">
        <div v-for="(l, i) in logs" :key="i" class="log-line">{{ l }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.kb {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
  padding: 16px 20px;
}
.kb-head {
  display: flex;
  align-items: baseline;
  gap: 14px;
}
.kb-head h2 {
  margin: 0;
}
.stat {
  color: #57606a;
  font-size: 13px;
}
.model-card {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border: 1px solid #d4a72c66;
  background: #fff8c5;
  border-radius: 8px;
  font-size: 13px;
}
.kb-main {
  display: flex;
  gap: 12px;
  flex: 1;
  min-height: 0;
}
.docs {
  flex: 0 0 260px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
}
.docs-head {
  display: flex;
  gap: 8px;
}
.docs-head button {
  flex: 1;
  padding: 6px 0;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
}
.docs-head button:disabled {
  opacity: 0.6;
  cursor: default;
}
.doc-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  background: #fff;
}
.doc {
  padding: 8px 10px;
  border-bottom: 1px solid #eaeef2;
}
.doc-title {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.doc-meta {
  color: #57606a;
  font-size: 12px;
  margin-top: 2px;
}
.doc-actions {
  margin-top: 4px;
  display: flex;
  justify-content: flex-end;
}
.doc-actions .mini {
  padding: 2px 10px;
  font-size: 12px;
  border: 1px solid #d0d7de;
  border-radius: 4px;
  background: #fff;
  cursor: pointer;
  color: #cf222e;
}
.doc-actions .mini:hover {
  background: #ffebe9;
}
.chat {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
}
.chat-msgs {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  background: #fff;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.msg {
  max-width: 92%;
  padding: 8px 12px;
  border-radius: 8px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.msg.user {
  align-self: flex-end;
  background: #ddf4ff;
}
.msg.assistant {
  align-self: flex-start;
  background: #f6f8fa;
  border: 1px solid #eaeef2;
}
.srcs {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.src {
  font-size: 12px;
  color: #57606a;
  background: #fff;
  border: 1px solid #eaeef2;
  border-radius: 6px;
  padding: 4px 8px;
}
.src-tag {
  color: #1f6feb;
  font-weight: 600;
}
.src-loc {
  color: #1a7f37;
}
.src-snip {
  display: block;
  margin-top: 2px;
  color: #8b949e;
}
.chat-input {
  display: flex;
  gap: 8px;
}
.chat-input select {
  flex: 0 0 130px;
  padding: 6px 8px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
}
.chat-input .q {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
}
.chat-input button {
  padding: 6px 14px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
  white-space: nowrap;
}
.chat-input button.primary {
  background: #1f6feb;
  border-color: #1f6feb;
  color: #fff;
}
.chat-input button:disabled {
  opacity: 0.6;
  cursor: default;
}
.hits {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.hits-title {
  margin: 0;
  color: #57606a;
  font-size: 13px;
}
.hit {
  border: 1px solid #eaeef2;
  border-radius: 6px;
  padding: 6px 10px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.hit-where {
  color: #1f6feb;
  font-size: 12px;
}
.hit-text {
  font-size: 13px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}
.empty {
  color: #8b949e;
  text-align: center;
  margin: 24px 8px;
  line-height: 1.7;
}
.error {
  color: #cf222e;
  margin: 0;
}
.ok {
  color: #1a7f37;
  margin: 0;
  font-size: 13px;
}
.log-panel {
  border: 1px solid #d0d7de;
  border-radius: 8px;
  background: #fff;
  flex: 0 0 auto;
  max-height: 120px;
  display: flex;
  flex-direction: column;
}
.log-head {
  padding: 4px 10px;
  border-bottom: 1px solid #eaeef2;
  background: #f6f8fa;
  font-size: 12px;
  color: #57606a;
}
.log-body {
  overflow-y: auto;
  padding: 4px 10px;
  font-size: 12px;
  color: #57606a;
}
</style>
