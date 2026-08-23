<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

interface V2pEnv {
  ffmpeg: boolean;
  model_dir: string;
  model_ready: boolean;
  engines: string[];
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
}

const mediaPath = ref("");
const engine = ref("fun_asr_nano");
const running = ref(false);
const progress = ref<V2pProgress | null>(null);
const result = ref("");
const error = ref("");
const env = ref<V2pEnv | null>(null);

const unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  unlisteners.push(
    await listen<V2pProgress>("v2p:progress", (e) => {
      progress.value = e.payload;
    }),
  );
  try {
    env.value = await invoke<V2pEnv>("v2p_check_env");
  } catch (e) {
    error.value = String(e);
  }
});

onUnmounted(() => {
  unlisteners.forEach((u) => u());
});

async function browseMedia() {
  const sel = await open({ multiple: false });
  if (typeof sel === "string") mediaPath.value = sel;
}

async function start() {
  error.value = "";
  result.value = "";
  progress.value = null;
  if (!mediaPath.value) {
    error.value = "请选择视频/音频文件";
    return;
  }
  running.value = true;
  try {
    const done = await invoke<V2pDone>("v2p_transcribe", {
      mediaPath: mediaPath.value,
      engine: engine.value,
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
</script>

<template>
  <div class="v2p">
    <h2>视频转 PDF</h2>

    <div v-if="env" class="env">
      <span>ffmpeg: {{ env.ffmpeg ? "✓ 就绪" : "✗ 未找到" }}</span>
      <span>模型: {{ env.model_ready ? "✓ 已下载" : "✗ 未下载" }}</span>
      <span class="hint" :title="env.model_dir">{{ env.model_dir }}</span>
    </div>

    <div class="row">
      <input
        class="path"
        v-model="mediaPath"
        placeholder="选择视频或音频文件…"
      />
      <button @click="browseMedia">选文件…</button>
    </div>

    <div class="row">
      <label>ASR 引擎</label>
      <select v-model="engine">
        <option value="fun_asr_nano">Fun-ASR-Nano (默认, 高质量)</option>
        <option value="paraformer">Paraformer (轻量)</option>
      </select>
      <button v-if="!running" class="primary" @click="start">开始转写</button>
      <button v-else class="danger" @click="cancel">取消</button>
    </div>

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

    <p class="note">
      说明: 转写为 CPU 离线处理, 视频时长 1 小时约需 1.5-2 小时转写。
      首次使用需下载模型 (~950MB, ModelScope 源)。
    </p>
  </div>
</template>

<style scoped>
.v2p {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
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
}
.env .hint {
  color: #8b949e;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 280px;
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
.row button {
  padding: 6px 14px;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  background: #fff;
  cursor: pointer;
  white-space: nowrap;
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
.note {
  color: #8b949e;
  font-size: 12px;
}
</style>
