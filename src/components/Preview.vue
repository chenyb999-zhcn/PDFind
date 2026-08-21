<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { convertFileSrc } from "@tauri-apps/api/core";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

const props = defineProps<{
  path: string;
  page: number;
  keyword: string;
  caseInsensitive: boolean;
  wholeWord: boolean;
}>();
const emit = defineEmits<{ close: [] }>();

let task: pdfjs.PDFDocumentLoadingTask | null = null;
let doc: pdfjs.PDFDocumentProxy | null = null;
let renderSeq = 0;
let observer: IntersectionObserver | null = null;
let pageEls: HTMLElement[] = [];
const renderedPages = new Set<number>();
let resizeObserver: ResizeObserver | null = null;
let resizeTimer: number | undefined;

const loading = ref(false);
const error = ref("");
const pageNum = ref(1); // 当前可见页(随滚动更新)
const pageCount = ref(0);
const pendingJump = ref(0); // 渲染完成后需要二次校准跳转的页
const bodyRef = ref<HTMLDivElement | null>(null);
const pagesRef = ref<HTMLDivElement | null>(null);

// 用第 1 页宽高比给所有占位块定高,避免渲染后滚动位置跳变
async function applyAspect(aspect: number) {
  for (const el of pageEls) {
    if (!el.classList.contains("done")) {
      el.style.height = Math.round(el.clientWidth * aspect) + "px";
    }
  }
}

function setupObserver() {
  const body = bodyRef.value;
  if (!body) return;
  observer = new IntersectionObserver(
    (entries) => {
      for (const en of entries) {
        if (en.isIntersecting) {
          renderPage(Number((en.target as HTMLElement).dataset.page));
        }
      }
    },
    { root: body, rootMargin: "900px 0px" },
  );
  for (const el of pageEls) observer.observe(el);
}

// 构造关键字正则(与后端 engine 同规则: 转义/整词/大小写)
function kwRegex(): RegExp | null {
  const kw = props.keyword.trim();
  if (!kw) return null;
  let src = kw.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (props.wholeWord) src = `\\b(?:${src})\\b`;
  try {
    return new RegExp(src, props.caseInsensitive ? "gi" : "g");
  } catch {
    return null;
  }
}

// 矩阵乘法(viewport.transform × item.transform), 避免依赖 pdfjs.Util 类型
function mul6(m: number[], t: number[]): number[] {
  return [
    m[0] * t[0] + m[2] * t[1],
    m[1] * t[0] + m[3] * t[1],
    m[0] * t[2] + m[2] * t[3],
    m[1] * t[2] + m[3] * t[3],
    m[0] * t[4] + m[2] * t[5] + m[4],
    m[1] * t[4] + m[3] * t[5] + m[5],
  ];
}

interface TextSeg {
  str: string;
  transform: number[];
  width: number;
  start: number;
  end: number;
  cum: number[]; // 逐字符累计测宽(未归一化), cum[i] = 前 i 个字符宽度之和
  msum: number; // 测宽总和
}

// 离屏 canvas 逐字符测宽: 数字/CJK 等混排时等宽假设误差大
const measureCanvas = document.createElement("canvas");
const measureCtx = measureCanvas.getContext("2d");

function charOffsets(str: string): { cum: number[]; msum: number } {
  const cum = new Array<number>(str.length + 1);
  cum[0] = 0;
  let sum = 0;
  if (measureCtx) {
    measureCtx.font = "100px sans-serif";
    for (let i = 0; i < str.length; i++) {
      sum += measureCtx.measureText(str[i]).width;
      cum[i + 1] = sum;
    }
  }
  if (sum <= 0) {
    // 测量不可用时退化为等宽
    for (let i = 0; i <= str.length; i++) cum[i] = i;
    sum = str.length;
  }
  return { cum, msum: sum };
}

// 关键字高亮: 取文本层拼接全页文本, 命中串按逐字等宽近似映射为页面矩形
async function highlightPage(n: number) {
  if (!doc) return;
  const el = pagesRef.value?.querySelector<HTMLElement>(`[data-page="${n}"]`);
  if (!el || !el.classList.contains("done")) return;
  el.querySelectorAll(".hl").forEach((h) => h.remove());
  const re = kwRegex();
  if (!re) return;
  let boxes = 0;
  try {
    const page = await doc.getPage(n);
    const canvas = el.querySelector("canvas");
    if (!canvas || !canvas.width || !canvas.height) return;
    const base = page.getViewport({ scale: 1 });
    const vp = page.getViewport({ scale: canvas.width / base.width });
    const tc = await page.getTextContent();

    let full = "";
    const segs: TextSeg[] = [];
    for (const it of tc.items) {
      if (!("str" in it) || typeof it.str !== "string" || !it.str) continue;
      // full 始终拼接保证偏移对齐; 仅有效几何信息的项参与矩形映射
      const start = full.length;
      full += it.str;
      if ("hasEOL" in it && it.hasEOL) full += "\n";
      if (
        Array.isArray(it.transform) &&
        it.transform.length === 6 &&
        typeof it.width === "number" &&
        Number.isFinite(it.width)
      ) {
        const { cum, msum } = charOffsets(it.str);
        segs.push({
          str: it.str,
          transform: it.transform,
          width: it.width,
          start,
          end: start + it.str.length,
          cum,
          msum,
        });
      }
    }

    let m: RegExpExecArray | null;
    while ((m = re.exec(full)) !== null) {
      if (m[0].length === 0) {
        re.lastIndex++;
        continue;
      }
      const s = m.index;
      const e = s + m[0].length;
      for (const seg of segs) {
        if (seg.end <= s || seg.start >= e) continue;
        const ls = Math.max(s, seg.start) - seg.start;
        const le = Math.min(e, seg.end) - seg.start;
        const tx = mul6(vp.transform, seg.transform);
        const h = Math.hypot(tx[2], tx[3]) || 10;
        // 段宽: PDF 宽度 × 视口缩放; 异常零宽时按字号估算兜底
        const segW =
          seg.width > 0 ? seg.width * vp.scale : h * 0.55 * seg.str.length;
        // 测宽归一化: 使累计测宽精确对齐段的真实宽度
        const k = segW / seg.msum;
        const xs = seg.cum[ls] * k;
        const xe = seg.cum[le] * k;
        const d = document.createElement("div");
        d.className = "hl";
        d.style.left = ((tx[4] + xs) / canvas.width) * 100 + "%";
        d.style.top = ((tx[5] - h) / canvas.height) * 100 + "%";
        d.style.width = ((xe - xs) / canvas.width) * 100 + "%";
        // 高度按字号 1.32 倍: 底边落在基线下约 32% 字高处, 红线避开字形下伸笔画不压字
        d.style.height = ((h * 1.32) / canvas.height) * 100 + "%";
        el.appendChild(d);
        boxes++;
      }
    }
    if (boxes === 0 && full.trim()) {
      // 排障线索: 文本已取到但零命中, 输出样本便于核对关键词与实际文本差异
      console.debug(
        `[PDFind] 第 ${n} 页零命中, 关键词 "${props.keyword}", 文本样本:`,
        full.slice(0, 120),
      );
    }
  } catch (e) {
    console.warn(`[PDFind] 第 ${n} 页高亮失败:`, e);
  }
}

async function renderPage(n: number) {
  if (!doc || renderedPages.has(n)) return;
  renderedPages.add(n);
  const d = doc;
  try {
    const page = await d.getPage(n);
    if (doc !== d) return;
    const el = pagesRef.value?.querySelector<HTMLElement>(
      `[data-page="${n}"]`,
    );
    if (!el) return;
    const canvas = el.querySelector("canvas");
    if (!canvas) return;
    const base = page.getViewport({ scale: 1 });
    const avail = el.clientWidth || 800;
    // 渲染前按本页真实宽高比精确定高, 消除与第 1 页尺寸差异导致的跳动
    if (!el.classList.contains("done")) {
      el.style.height = Math.round((avail * base.height) / base.width) + "px";
    }
    // 乘 devicePixelRatio: 高分屏/窗口放大重渲后依然锐利
    const dpr = Math.min(Math.max(window.devicePixelRatio || 1, 1), 2);
    const scale = Math.min(4, Math.max(0.5, (avail * dpr) / base.width));
    const vp = page.getViewport({ scale });
    // 离屏渲染完成后原子拷贝, 渲染期间旧画面保持显示, 不闪黑
    const off = document.createElement("canvas");
    off.width = Math.round(vp.width);
    off.height = Math.round(vp.height);
    await page.render({ canvas: off, viewport: vp }).promise;
    if (doc !== d) return;
    canvas.width = off.width;
    canvas.height = off.height;
    canvas.getContext("2d")?.drawImage(off, 0, 0);
    el.style.height = "auto";
    el.classList.add("done");
    highlightPage(n);
    // 之前发起的跳页目标渲染完成后校准一次滚动位置
    if (pendingJump.value === n) {
      pendingJump.value = 0;
      scrollToPage(n, true);
    }
  } catch {
    renderedPages.delete(n);
  }
}

function scrollToPage(n: number, immediate = false) {
  const body = bodyRef.value;
  const el = pagesRef.value?.querySelector<HTMLElement>(`[data-page="${n}"]`);
  if (!body || !el) return;
  const top =
    el.getBoundingClientRect().top -
    body.getBoundingClientRect().top +
    body.scrollTop -
    6;
  body.scrollTo({ top, behavior: immediate ? "auto" : "smooth" });
  pageNum.value = n;
}

function jumpTo(n: number) {
  if (!pageCount.value || n < 1 || n > pageCount.value) return;
  const el = pagesRef.value?.querySelector(`[data-page="${n}"]`);
  if (el && !(el as HTMLElement).classList.contains("done")) {
    pendingJump.value = n; // 未渲染,先标记等 renderPage 校准
  }
  scrollToPage(n);
}

// 滚动时更新当前页码指示
function onScroll() {
  const body = bodyRef.value;
  if (!body || !pageEls.length) return;
  const top = body.getBoundingClientRect().top;
  for (const el of pageEls) {
    if (el.getBoundingClientRect().bottom > top + 40) {
      const n = Number(el.dataset.page);
      if (n && n !== pageNum.value) pageNum.value = n;
      break;
    }
  }
}

// 面板宽度变化(拖分隔条/缩放窗口)后按新宽度重渲已显示页面, 防止画布被拉伸发糊
function rerenderAll() {
  if (!doc) return;
  for (const el of pageEls) {
    if (!el.classList.contains("done")) continue;
    // 冻结当前高度占位, 避免重渲期间滚动条跳动
    el.style.height = el.getBoundingClientRect().height + "px";
    el.classList.remove("done");
    // 不清空 canvas: 旧画面保持显示, 待离屏渲染完成原子替换, 无黑窗
    const n = Number(el.dataset.page);
    if (n) renderedPages.delete(n);
  }
  // 重建观察器: 对当前可视页面立即触发重渲(不可见的保持懒加载)
  observer?.disconnect();
  observer = null;
  setupObserver();
}

async function load() {
  const seq = ++renderSeq;
  error.value = "";
  loading.value = true;
  renderedPages.clear();
  observer?.disconnect();
  observer = null;
  pageEls = [];
  pageCount.value = 0;
  const oldTask = task;
  doc = null;
  task = null;
  try {
    const t = pdfjs.getDocument({ url: convertFileSrc(props.path) });
    task = t;
    const d = await t.promise;
    if (seq !== renderSeq) {
      await t.destroy();
      return;
    }
    doc = d;
    pageCount.value = d.numPages;
    await oldTask?.destroy();
    await nextTick();
    pageEls = Array.from(
      pagesRef.value?.querySelectorAll<HTMLElement>(".pwrap") ?? [],
    );
    const p1 = await d.getPage(1);
    const v1 = p1.getViewport({ scale: 1 });
    await applyAspect(v1.height / v1.width);
    setupObserver();
    if (pagesRef.value && !resizeObserver) {
      let lastW = 0;
      resizeObserver = new ResizeObserver((entries) => {
        const w = Math.round(
          entries[entries.length - 1]?.contentRect.width ?? 0,
        );
        if (!lastW) {
          lastW = w; // 首次回调只记基线
          return;
        }
        if (w === lastW) return; // 高度变化(新页渲染沉淀)忽略, 防整屏重渲闪黑
        lastW = w;
        clearTimeout(resizeTimer);
        resizeTimer = window.setTimeout(rerenderAll, 200);
      });
      resizeObserver.observe(pagesRef.value);
    }
    scrollToPage(Math.min(Math.max(1, props.page), d.numPages), true);
  } catch (e) {
    if (seq === renderSeq) {
      error.value = `无法打开 PDF: ${e instanceof Error ? e.message : String(e)}`;
    }
  } finally {
    if (seq === renderSeq) loading.value = false;
  }
}

watch(
  () => props.page,
  (n) => {
    if (doc) jumpTo(n);
  },
);

// 关键字/选项变化时对已渲染页面重算高亮
watch(
  () => [props.keyword, props.caseInsensitive, props.wholeWord],
  () => {
    for (const el of pageEls) {
      if (el.classList.contains("done")) highlightPage(Number(el.dataset.page));
    }
  },
);

onMounted(load);
onUnmounted(() => {
  renderSeq++;
  observer?.disconnect();
  resizeObserver?.disconnect();
  clearTimeout(resizeTimer);
  doc = null;
  task?.destroy();
  task = null;
});
</script>

<template>
  <aside class="preview">
    <header class="phead">
      <span class="ptitle" :title="path">
        {{
          path.split(/[\\/]/).pop() || path
        }}
      </span>
      <div class="pnav">
        <button :disabled="pageNum <= 1" @click="jumpTo(pageNum - 1)">
          ‹
        </button>
        <span class="pnum">{{ pageNum }} / {{ pageCount || "…" }}</span>
        <button
          :disabled="!pageCount || pageNum >= pageCount"
          @click="jumpTo(pageNum + 1)"
        >
          ›
        </button>
      </div>
      <button class="pclose" @click="emit('close')">✕ 关闭</button>
    </header>
    <div ref="bodyRef" class="pbody" @scroll.passive="onScroll">
      <p v-if="error" class="perr">{{ error }}</p>
      <div v-else ref="pagesRef" class="pages">
        <div v-for="n in pageCount" :key="n" class="pwrap" :data-page="n">
          <canvas />
          <span class="plabel">{{ n }}</span>
        </div>
      </div>
      <div v-if="loading" class="pload">加载中…</div>
    </div>
  </aside>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  min-width: 0;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  background: #fff;
  overflow: hidden;
  animation: slidein 0.18s ease;
}
@keyframes slidein {
  from {
    transform: translateX(24px);
    opacity: 0;
  }
}
.phead {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-bottom: 1px solid #d0d7de;
  background: #f6f8fa;
}
.ptitle {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  flex: 1;
}
.pnav {
  display: flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
}
.pnav button,
.pclose {
  padding: 3px 10px;
  border: 1px solid #d0d7de;
  border-radius: 5px;
  background: #fff;
  cursor: pointer;
  font-size: 13px;
}
.pnav button:disabled {
  opacity: 0.5;
  cursor: default;
}
.pnum {
  color: #57606a;
  min-width: 56px;
  text-align: center;
}
.pclose {
  color: #cf222e;
}
.pbody {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  background: #525659;
  position: relative;
  overscroll-behavior: contain;
}
.pages {
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.pwrap {
  position: relative;
  min-height: 200px;
  background: #e3e6ea;
  border-radius: 2px;
}
.pwrap canvas {
  display: block;
  width: 100%;
}
.pwrap :deep(.hl) {
  position: absolute;
  background: transparent;
  border-bottom: 3px solid #e5484d;
  border-radius: 0;
  pointer-events: none;
  box-sizing: border-box;
}
.plabel {
  position: absolute;
  bottom: -18px;
  left: 50%;
  transform: translateX(-50%);
  color: #c9d1d9;
  font-size: 12px;
}
.perr {
  color: #fff;
  align-self: center;
}
.pload {
  position: absolute;
  top: 10px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(0, 0, 0, 0.65);
  color: #fff;
  padding: 4px 14px;
  border-radius: 12px;
  font-size: 13px;
}
</style>
