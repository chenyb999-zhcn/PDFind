<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
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
  base_url: string;
  default_model: string;
  needs_model: boolean;
  has_key: boolean;
  models: string[];
}
interface OrganizerConfig {
  keys: Record<string, string>;
  models: Record<string, string>;
  custom: { base_url: string; model: string; api_key: string };
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
// 当前选中服务商的 Key/Model 输入 (编辑后保存到后端配置)
const apiKeyInput = ref("");
const modelInput = ref("");
const modelManual = ref(false);   // Model 下拉是否处于"手动输入"模式
const modelOptions = ref<string[]>([]); // Model 下拉选项
const fetchingModels = ref(false);
const customBaseUrl = ref("");
const customModel = ref("");
const customKey = ref("");

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
    // 加载整理服务商配置
    await loadOrganizerConfig();
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

const selectedOrganizer = computed(() =>
  env.value?.organizers.find((o) => o.id === organizerId.value),
);

// 当前服务商选中时填充 Key/Model 输入框
function fillOrganizerInputs() {
  const sel = selectedOrganizer.value;
  if (!sel) return;
  if (sel.id === "custom") {
    customBaseUrl.value = orgCfg.value.custom.base_url || "";
    customModel.value = orgCfg.value.custom.model || "";
    customKey.value = orgCfg.value.custom.api_key || "";
  } else {
    apiKeyInput.value = orgCfg.value.keys[sel.id] || "";
    // Model 下拉选项: 预置列表 + 已保存值(去重)
    const saved = orgCfg.value.models[sel.id] || "";
    const opts = [...sel.models];
    if (saved && !opts.includes(saved)) opts.push(saved);
    modelOptions.value = opts;
    // 已保存值在预置列表中→正常下拉; 否则→手动输入模式
    if (saved && sel.models.length && !sel.models.includes(saved)) {
      modelManual.value = true;
      modelInput.value = saved;
    } else {
      modelManual.value = false;
      modelInput.value = saved || sel.default_model;
    }
  }
}

// 服务商切换时刷新输入框
watch(organizerId, () => {
  fillOrganizerInputs();
});

let orgCfg = ref<OrganizerConfig>({
  keys: {},
  models: {},
  custom: { base_url: "", model: "", api_key: "" },
});

async function loadOrganizerConfig() {
  try {
    orgCfg.value = await invoke<OrganizerConfig>("v2p_get_organizer_config");
  } catch (e) {
    console.error("加载配置失败", e);
  }
  fillOrganizerInputs();
}

async function saveOrganizerConfig() {
  const sel = selectedOrganizer.value;
  if (!sel) return;
  if (sel.id === "custom") {
    orgCfg.value.custom = {
      base_url: customBaseUrl.value.trim(),
      model: customModel.value.trim(),
      api_key: customKey.value.trim(),
    };
  } else {
    orgCfg.value.keys[sel.id] = apiKeyInput.value.trim();
    if (modelInput.value.trim() && modelInput.value.trim() !== sel.default_model) {
      orgCfg.value.models[sel.id] = modelInput.value.trim();
    } else {
      delete orgCfg.value.models[sel.id];
    }
  }
  try {
    await invoke("v2p_set_organizer_config", { config: orgCfg.value });
    pdfMsg.value = "配置已保存";
    env.value = await invoke<V2pEnv>("v2p_check_env");
  } catch (e) {
    error.value = String(e);
  }
}

// 动态拉取服务商模型列表
async function fetchOrganizerModels() {
  const sel = selectedOrganizer.value;
  if (!sel) return;
  fetchingModels.value = true;
  pdfMsg.value = "";
  try {
    const list = await invoke<string[]>("v2p_list_organizer_models", {
      providerId: sel.id,
    });
    modelOptions.value = list;
    if (!modelInput.value || !list.includes(modelInput.value)) {
      modelManual.value = true; // 当前值不在列表中, 保持手动输入
    }
    pdfMsg.value = `获取到 ${list.length} 个模型`;
  } catch (e) {
    error.value = String(e);
  } finally {
    fetchingModels.value = false;
  }
}

// Model 下拉选择: 选真实模型则退出手动模式, 选手动输入则切手动
function onModelSelect(e: Event) {
  const v = (e.target as HTMLSelectElement).value;
  if (v === "__manual__") {
    modelManual.value = true;
    return;
  }
  modelInput.value = v;
  modelManual.value = false;
}

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
      <label>整理服务商</label>
      <select v-model="organizerId" class="grow">
        <option v-for="o in env.organizers" :key="o.id" :value="o.id">
          {{ o.name }} {{ o.has_key ? "✓" : "" }}
        </option>
      </select>
    </div>

    <div v-if="selectedOrganizer && selectedOrganizer.id !== 'custom'" class="row">
      <label>API Key</label>
      <input
        class="path"
        type="password"
        v-model="apiKeyInput"
        :placeholder="selectedOrganizer.has_key ? '已配置, 输入可修改' : '粘贴 API Key'"
      />
    </div>

    <div v-if="selectedOrganizer && selectedOrganizer.id !== 'custom'" class="row">
      <label>Model</label>
      <select
        v-if="!modelManual"
        class="grow"
        :value="modelInput"
        @change="onModelSelect($event)"
      >
        <option v-for="m in modelOptions" :key="m" :value="m">{{ m }}</option>
        <option value="__manual__">✏️ 手动输入…</option>
      </select>
      <button v-if="!modelManual" @click="modelManual = true">手动</button>
      <input
        v-else
        class="path"
        v-model="modelInput"
        :placeholder="selectedOrganizer.needs_model ? '填 Endpoint ID/模型 ID' : '输入模型名'"
      />
      <button
        :disabled="fetchingModels || !apiKeyInput.trim()"
        @click="fetchOrganizerModels"
        :title="apiKeyInput.trim() ? '从服务商拉取模型列表' : '先填 API Key'"
      >
        {{ fetchingModels ? "获取中…" : "获取列表" }}
      </button>
    </div>

    <div v-if="selectedOrganizer && selectedOrganizer.id === 'custom'" class="row">
      <label>Base URL</label>
      <input class="path" v-model="customBaseUrl" placeholder="https://api.example.com/v1" />
    </div>
    <div v-if="selectedOrganizer && selectedOrganizer.id === 'custom'" class="row">
      <label>Model</label>
      <input class="path" v-model="customModel" placeholder="模型名" />
    </div>
    <div v-if="selectedOrganizer && selectedOrganizer.id === 'custom'" class="row">
      <label>API Key</label>
      <input class="path" type="password" v-model="customKey" placeholder="粘贴 API Key" />
    </div>

    <div v-if="selectedOrganizer" class="row">
      <button @click="saveOrganizerConfig">保存配置</button>
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
