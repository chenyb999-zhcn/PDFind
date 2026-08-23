<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

interface ModelInfo {
  id: string;
  name: string;
  runtime: string;
  gpu: boolean;
  needs_vad: boolean;
  downloaded: boolean;
}
interface DeviceInfo {
  has_nvidia_gpu: boolean;
  gpu_name: string;
  available: string[];
}
interface V2pEnv {
  ffmpeg: boolean;
  models: ModelInfo[];
  device: DeviceInfo;
  catalog_version: number;
  has_update: boolean;
  organizers: OrganizerInfo[];
}
interface OrganizerInfo {
  id: string;
  name: string;
  downloaded: boolean;
  size_mb: number;
}
interface V2pProgress {
  stage: string;
  done: number;
  total: number;
  current: string;
}
interface V2pDone {
  segments: number;
  chars: number;
  segment_list: Segment[];
}
interface Segment {
  text: string;
  start: number;
  end: number;
}
interface DlProgress {
  model_id: string;
  file: string;
  done_bytes: number;
  total_bytes: number;
  current_file_idx: number;
  total_files: number;
}

const mediaPath = ref("");
const modelId = ref("");
const device = ref("auto");
const lang = ref("zh");
const running = ref(false);
const progress = ref<V2pProgress | null>(null);
const logs = ref<string[]>([]);
const error = ref("");
const result = ref("");
const resultSegs = ref<Segment[]>([]);
const env = ref<V2pEnv | null>(null);
const downloadingId = ref<string | null>(null);
const dlMsg = ref("");
const updateMsg = ref("");
const generatingPdf = ref(false);
const pdfMsg = ref("");
const organizePdf = ref(false);
const organizerId = ref("");
const downloadingOrg = ref<string | null>(null);

const LANG_OPTS = [
  { value: "zh", label: "中文" },
  { value: "en", label: "English" },
  { value: "ja", label: "日本語" },
];

const unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  unlisteners.push(
    await listen<V2pProgress>("v2p:progress", (e) => {
      progress.value = e.payload;
    }),
  );
  unlisteners.push(
    await listen<{ text: string }>("v2p:log", (e) => {
      const now = new Date();
      const ts = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}:${String(now.getSeconds()).padStart(2, "0")}`;
      logs.value.push(`[${ts}] ${e.payload.text}`);
      if (logs.value.length > 1000) logs.value.shift();
    }),
  );
  unlisteners.push(
    await listen<DlProgress>("v2p:dl", (e) => {
      const p = e.payload;
      dlMsg.value = `下载中: ${p.file} (${fmtBytes(p.done_bytes)}/${fmtBytes(p.total_bytes)})`;
    }),
  );
  unlisteners.push(
    await listen<Segment[]>("v2p:result", (e) => {
      resultSegs.value = e.payload;
    }),
  );
  try {
    env.value = await invoke<V2pEnv>("v2p_check_env");
    if (env.value.models.length) modelId.value = env.value.models[0].id;
    if (env.value.organizers.length) organizerId.value = env.value.organizers[0].id;
    // 设备自动检测
    if (env.value.device.available.includes("cuda")) {
      device.value = "auto";
    }
    // 检查更新
    const upd = await invoke<{ has_update: boolean; new_version: number }>(
      "v2p_check_update",
    );
    if (upd.has_update)
      updateMsg.value = `发现新模型清单 v${upd.new_version} (当前 v${env.value.catalog_version})`;
  } catch (e) {
    error.value = String(e);
  }
});

onUnmounted(() => {
  unlisteners.forEach((u) => u());
});

function fmtBytes(b: number): string {
  if (b > 1024 * 1024 * 1024) return (b / 1024 / 1024 / 1024).toFixed(1) + " GB";
  return (b / 1024 / 1024).toFixed(0) + " MB";
}

async function browseMedia() {
  const sel = await open({ multiple: false });
  if (typeof sel === "string") mediaPath.value = sel;
}

async function start() {
  error.value = "";
  result.value = "";
  progress.value = null;
  logs.value = [];
  resultSegs.value = [];
  pdfMsg.value = "";
  if (!mediaPath.value) {
    error.value = "请选择视频/音频文件";
    return;
  }
  if (!modelId.value) {
    error.value = "请选择 ASR 模型";
    return;
  }
  running.value = true;
  try {
    const done = await invoke<V2pDone>("v2p_transcribe", {
      mediaPath: mediaPath.value,
      modelId: modelId.value,
      device: device.value,
      lang: lang.value,
    });
    result.value = `转写完成: ${done.chars} 字符, ${done.segments} 段`;
  } catch (e) {
    if (!String(e).includes("已取消")) error.value = String(e);
  } finally {
    running.value = false;
    progress.value = null;
  }
}

async function cancel() {
  try {
    await invoke("cancel_search");
  } catch {
    /* ignore */
  }
}

async function downloadModel(id: string) {
  downloadingId.value = id;
  dlMsg.value = "";
  try {
    await invoke("v2p_download_model", { modelId: id });
    dlMsg.value = "下载完成";
    env.value = await invoke<V2pEnv>("v2p_check_env");
  } catch (e) {
    error.value = String(e);
  } finally {
    downloadingId.value = null;
  }
}

async function downloadOrganizer(id: string) {
  downloadingOrg.value = id;
  dlMsg.value = "";
  try {
    await invoke("v2p_download_organizer", { organizerId: id });
    dlMsg.value = "整理模型下载完成";
    env.value = await invoke<V2pEnv>("v2p_check_env");
  } catch (e) {
    error.value = String(e);
  } finally {
    downloadingOrg.value = null;
  }
}

const selectedOrganizer = computed(() =>
  env.value?.organizers.find((o) => o.id === organizerId.value),
);

function fmtTs(s: number): string {
  if (!isFinite(s) || s <= 0) return "";
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  return `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

async function generatePdf() {
  if (!mediaPath.value) {
    error.value = "请先选择视频文件";
    return;
  }
  const out = await save({
    title: "保存 PDF",
    defaultPath: "视频转PDF.pdf",
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (!out) return;
  generatingPdf.value = true;
  pdfMsg.value = "";
  try {
    // 拼接转写全文(用于 LLM 整理)
    const transcript = resultSegs.value.map((s) => s.text).join("\n");
    await invoke("v2p_generate_pdf", {
      mediaPath: mediaPath.value,
      outPdf: out,
      lang: lang.value,
      transcript,
      segments: resultSegs.value,
      organize: organizePdf.value,
      organizerId: organizerId.value,
    });
    pdfMsg.value = `PDF 已生成: ${out}`;
  } catch (e) {
    error.value = String(e);
  } finally {
    generatingPdf.value = false;
  }
}

// 所选模型的 GPU 支持
const selectedModel = computed(() =>
  env.value?.models.find((m) => m.id === modelId.value),
);
// 有效设备选项
const deviceOptions = computed(() => {
  const opts: { value: string; label: string; disabled?: boolean }[] = [
    { value: "auto", label: "自动检测" },
    { value: "cpu", label: "CPU" },
  ];
  const hasGpu = env.value?.device.has_nvidia_gpu ?? false;
  const gpuOk = selectedModel.value?.gpu ?? false;
  opts.push({
    value: "cuda",
    label: hasGpu
      ? `GPU (${env.value?.device.gpu_name ?? "CUDA"})`
      : "GPU (未检测到)",
    disabled: !hasGpu || !gpuOk,
  });
  return opts;
});
</script>

<template>
  <div class="v2p">
    <h2>视频转 PDF</h2>

    <div v-if="env" class="env">
      <span>ffmpeg: {{ env.ffmpeg ? "✓ 就绪" : "✗ 未找到" }}</span>
      <span>GPU: {{ env.device.has_nvidia_gpu ? env.device.gpu_name : "无" }}</span>
      <span v-if="updateMsg" class="upd">🛈 {{ updateMsg }}</span>
    </div>

    <div class="row">
      <input class="path" v-model="mediaPath" placeholder="选择视频或音频文件…" />
      <button @click="browseMedia">选文件…</button>
    </div>

    <div class="row">
      <label>ASR 模型</label>
      <select v-model="modelId" class="grow">
        <option v-for="m in env?.models" :key="m.id" :value="m.id">
          {{ m.name }} {{ m.downloaded ? "✓" : "✗" }}
        </option>
      </select>
      <button
        v-if="selectedModel && !selectedModel.downloaded"
        :disabled="downloadingId !== null"
        @click="downloadModel(selectedModel.id)"
      >
        {{ downloadingId === selectedModel.id ? "下载中…" : "下载" }}
      </button>
    </div>

    <div class="row">
      <label>计算设备</label>
      <select v-model="device">
        <option
          v-for="o in deviceOptions"
          :key="o.value"
          :value="o.value"
          :disabled="o.disabled"
        >
          {{ o.label }}
        </option>
      </select>
      <span v-if="selectedModel && !selectedModel.gpu && device === 'cuda'" class="warn">
        该模型不支持 CUDA，将回退 CPU
      </span>
    </div>

    <div class="row">
      <label>识别语言</label>
      <select v-model="lang">
        <option v-for="o in LANG_OPTS" :key="o.value" :value="o.value">
          {{ o.label }}
        </option>
      </select>
      <span v-if="selectedModel && selectedModel.runtime !== 'llamacpp'" class="warn">
        语言设置仅对 Fun-ASR-Nano(llama.cpp) 生效
      </span>
    </div>

    <div class="row">
      <button v-if="!running" class="primary" @click="start">开始转写</button>
      <button v-else class="danger" @click="cancel">取消</button>
      <button
        v-if="!running && resultSegs.length"
        :disabled="generatingPdf"
        @click="generatePdf"
      >
        {{ generatingPdf ? "生成中…" : "生成 PDF" }}
      </button>
      <label v-if="resultSegs.length" class="chk">
        <input type="checkbox" v-model="organizePdf" />
        本地 LLM 整理
      </label>
    </div>

    <div v-if="organizePdf && env?.organizers.length" class="row">
      <label>整理模型</label>
      <select v-model="organizerId" class="grow">
        <option v-for="o in env.organizers" :key="o.id" :value="o.id">
          {{ o.name }} {{ o.downloaded ? "✓" : "✗" }}
        </option>
      </select>
      <button
        v-if="selectedOrganizer && !selectedOrganizer.downloaded"
        :disabled="downloadingOrg !== null"
        @click="downloadOrganizer(selectedOrganizer.id)"
      >
        {{ downloadingOrg === selectedOrganizer.id ? "下载中…" : "下载" }}
      </button>
    </div>

    <p v-if="dlMsg" class="ok">{{ dlMsg }}</p>

    <div v-if="progress" class="progress">
      <div class="bar">
        <div
          class="fill"
          :style="{
            width: progress.total ? (progress.done / progress.total) * 100 + '%' : '0%',
          }"
        />
      </div>
      <span class="ptext">{{ progress.current }}</span>
    </div>

    <p v-if="error" class="error">{{ error }}</p>
    <p v-if="result" class="ok">{{ result }}</p>
    <p v-if="pdfMsg" class="ok">{{ pdfMsg }}</p>

    <div v-if="resultSegs.length" class="log-panel">
      <div class="log-head">转写结果 ({{ resultSegs.length }} 段)</div>
      <div class="log-body result-body">
        <div v-for="(s, i) in resultSegs" :key="i" class="result-seg">
          <span class="ts" v-if="fmtTs(s.start)">[{{ fmtTs(s.start) }} - {{ fmtTs(s.end) }}]</span>
          {{ s.text }}
        </div>
      </div>
    </div>

    <div v-if="logs.length" class="log-panel">
      <div class="log-head">转写日志 ({{ logs.length }})</div>
      <div class="log-body log-body-short">
        <div v-for="(l, i) in logs" :key="i" class="log-line">{{ l }}</div>
      </div>
    </div>

    <p class="note">
      说明: 转写为本地离线处理。SenseVoice 与 Fun-ASR-Nano 支持 GPU(CUDA) 加速;
      Paraformer 仅 CPU。首次使用需下载模型。
    </p>
  </div>
</template>

<style scoped>
.v2p {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 720px;
  margin: 0 auto;
  padding: 24px;
}
.v2p h2 {
  margin: 0;
}
.env {
  display: flex;
  gap: 16px;
  font-size: 13px;
  color: #57606a;
  flex-wrap: wrap;
}
.env .upd {
  color: #9a6700;
}
.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.row .path {
  flex: 1;
  padding: 6px 10px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
}
.row .grow {
  flex: 1;
}
.row label {
  white-space: nowrap;
  color: #57606a;
}
.row button {
  padding: 6px 14px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
  white-space: nowrap;
}
.row button:disabled {
  opacity: 0.5;
  cursor: default;
}
.row button.primary {
  background: #1a7f37;
  border-color: #1a7f37;
  color: #fff;
}
.row button.danger {
  background: #cf222e;
  border-color: #cf222e;
  color: #fff;
}
.row select {
  padding: 6px 8px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
}
.warn {
  color: #cf222e;
  font-size: 12px;
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
  font-size: 13px;
}
.error {
  color: #cf222e;
  margin: 0;
}
.ok {
  color: #1a7f37;
  margin: 0;
}
.log-panel {
  border: 1px solid #d0d7de;
  border-radius: 8px;
  overflow: hidden;
}
.log-head {
  padding: 6px 12px;
  background: #f6f8fa;
  border-bottom: 1px solid #d0d7de;
  font-size: 13px;
  color: #57606a;
}
.log-body {
  max-height: 93px; /* 日志高度: 原来的 1/3 (280px / 3 ≈ 93px) */
  overflow-y: auto;
  padding: 8px 12px;
  background: #f6f8fa;
}
.result-body {
  max-height: 280px;
}
.log-body-short {
  max-height: 93px;
}
.result-seg {
  padding: 3px 0;
  line-height: 1.5;
  color: #1f2328;
  word-break: break-all;
}
.result-seg .ts {
  color: #57606a;
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: 12px;
  margin-right: 4px;
}
.log-line {
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: 12px;
  padding: 2px 0;
  color: #1f2328;
  word-break: break-all;
}
.chk {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 13px;
  color: #57606a;
  white-space: nowrap;
  cursor: pointer;
}
.note {
  color: #8b949e;
  font-size: 12px;
}
</style>
