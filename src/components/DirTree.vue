<script setup lang="ts">
// 左侧目录树: 资源管理器风格, 只显示子目录与 PDF, 懒加载展开, 点击只填路径不搜索
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface TreeEntry {
  name: string;
  path: string;
  is_dir: boolean;
  has_children: boolean;
}

const emit = defineEmits<{ pick: [path: string, isDir: boolean] }>();

const LS_ROOT = "dirtree.root";
const LS_EXPANDED = "dirtree.expanded";

const root = ref("");
const err = ref("");
const children = ref(new Map<string, TreeEntry[]>());
const expanded = ref(new Set<string>());
const loading = ref(new Set<string>());
const selected = ref("");

const rootEntries = computed(() =>
  root.value ? children.value.get(root.value) ?? null : null,
);

interface Row {
  entry: TreeEntry;
  depth: number;
  busy: boolean;
}

// 展开集合 + 子项缓存 → 扁平行序列(带缩进深度), 避免递归组件
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  const walk = (list: TreeEntry[] | null, depth: number) => {
    if (!list) return;
    for (const e of list) {
      const open = e.is_dir && expanded.value.has(e.path);
      const busy = open && !children.value.has(e.path);
      out.push({ entry: e, depth, busy });
      if (open && !busy) walk(children.value.get(e.path) ?? null, depth + 1);
    }
  };
  walk(rootEntries.value, 0);
  return out;
});

async function loadChildren(dir: string): Promise<boolean> {
  if (children.value.has(dir)) return true;
  loading.value.add(dir);
  err.value = "";
  try {
    const list = await invoke<TreeEntry[]>("list_tree_dir", { path: dir });
    children.value.set(dir, list);
    return true;
  } catch (e) {
    err.value = String(e);
    expanded.value.delete(dir);
    return false;
  } finally {
    loading.value.delete(dir);
  }
}

async function toggle(e: TreeEntry) {
  if (!e.is_dir || !e.has_children) return;
  if (expanded.value.has(e.path)) {
    expanded.value.delete(e.path);
  } else {
    expanded.value.add(e.path);
    await loadChildren(e.path);
  }
  persistExpanded();
}

function pick(e: TreeEntry) {
  selected.value = e.path;
  emit("pick", e.path, e.is_dir);
}

async function chooseRoot() {
  const sel = await open({ multiple: false, directory: true });
  if (typeof sel === "string") await setRoot(sel);
}

// 设置根目录并加载; restore 为需要恢复展开的目录列表(父目录在前)
async function setRoot(p: string, restore: string[] = []) {
  root.value = p;
  children.value = new Map();
  expanded.value = new Set();
  selected.value = "";
  err.value = "";
  persistRoot();
  if (!(await loadChildren(p))) return;
  for (const dir of restore) {
    expanded.value.add(dir);
    await loadChildren(dir);
  }
  persistExpanded();
}

async function refresh() {
  if (root.value) await setRoot(root.value, [...expanded.value]);
}

function collapseAll() {
  expanded.value.clear();
  persistExpanded();
}

function basename(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() || p;
}

function persistRoot() {
  try {
    localStorage.setItem(LS_ROOT, root.value);
  } catch {
    /* 存储不可用时忽略 */
  }
}

function persistExpanded() {
  try {
    localStorage.setItem(LS_EXPANDED, JSON.stringify([...expanded.value]));
  } catch {
    /* ignore */
  }
}

// 启动时还原上次根目录与展开状态
onMounted(() => {
  let savedRoot = "";
  let savedExpanded: string[] = [];
  try {
    savedRoot = localStorage.getItem(LS_ROOT) ?? "";
    const raw = localStorage.getItem(LS_EXPANDED);
    const parsed = raw ? JSON.parse(raw) : [];
    if (Array.isArray(parsed))
      savedExpanded = parsed.filter((x): x is string => typeof x === "string");
  } catch {
    /* ignore */
  }
  if (savedRoot) void setRoot(savedRoot, savedExpanded);
});
</script>

<template>
  <aside class="dtree">
    <header class="dhead">
      <span class="dtitle">目录</span>
      <span class="dspacer" />
      <button title="选择根目录" @click="chooseRoot">根目录</button>
      <button title="重新加载" :disabled="!root" @click="refresh">刷新</button>
      <button
        title="全部折叠"
        :disabled="!expanded.size"
        @click="collapseAll"
      >
        折叠
      </button>
    </header>

    <p v-if="root" class="droot" :title="root">{{ basename(root) }}</p>
    <p v-if="err" class="derr">{{ err }}</p>

    <div v-if="!root" class="dempty">
      <p>选择一个根目录，<br />浏览其中的子目录与 PDF 文件</p>
      <button class="primary" @click="chooseRoot">选择根目录…</button>
    </div>

    <div v-else class="dbody">
      <div
        v-for="row in rows"
        :key="row.entry.path"
        class="drow"
        :class="{ sel: selected === row.entry.path }"
        :style="{ paddingLeft: 6 + row.depth * 16 + 'px' }"
        :title="row.entry.path"
        @click="pick(row.entry)"
        @dblclick="row.entry.is_dir && toggle(row.entry)"
      >
        <span
          class="twisty"
          :class="{
            open: expanded.has(row.entry.path),
            off: !row.entry.is_dir || !row.entry.has_children,
          }"
          @click.stop="toggle(row.entry)"
        >
          <span v-if="row.busy" class="spin" />
          <svg v-else viewBox="0 0 16 16" width="12" height="12">
            <path
              d="M6 3.5 10.5 8 6 12.5"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </span>

        <svg
          v-if="row.entry.is_dir && expanded.has(row.entry.path)"
          class="dicon"
          viewBox="0 0 16 16"
          width="16"
          height="16"
        >
          <path
            d="M1.5 4A1.5 1.5 0 0 1 3 2.5h3l1.4 1.8H13A1.5 1.5 0 0 1 14.5 6v1H4.6L2 13.3A1.5 1.5 0 0 1 1.5 12V4z"
            fill="#ffd257"
            stroke="#d9a514"
            stroke-width=".8"
          />
          <path
            d="M3 13.5h10.7a1 1 0 0 0 .96-.72L16 7.5H4.9L2.9 12.9a1 1 0 0 0 .1.6z"
            fill="#ffdf80"
            stroke="#d9a514"
            stroke-width=".8"
          />
        </svg>
        <svg
          v-else-if="row.entry.is_dir"
          class="dicon"
          viewBox="0 0 16 16"
          width="16"
          height="16"
        >
          <path
            d="M1.5 4A1.5 1.5 0 0 1 3 2.5h3.1c.4 0 .79.16 1.07.44l1.1 1.1H13A1.5 1.5 0 0 1 14.5 6v6A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V4z"
            fill="#ffd257"
            stroke="#d9a514"
            stroke-width=".8"
          />
        </svg>
        <svg v-else class="dicon" viewBox="0 0 16 16" width="16" height="16">
          <path
            d="M4 1.5h5.2L12.8 5v9.2a.8.8 0 0 1-.8.8H4a.8.8 0 0 1-.8-.8V2.3a.8.8 0 0 1 .8-.8z"
            fill="#fff"
            stroke="#b9c2cc"
            stroke-width=".8"
          />
          <path
            d="M9.2 1.8v3.5h3.4"
            fill="none"
            stroke="#b9c2cc"
            stroke-width=".8"
          />
          <rect x="5" y="8.2" width="6" height="1.3" rx=".65" fill="#e5484d" />
          <rect x="5" y="10.8" width="6" height="1.3" rx=".65" fill="#e5484d" />
        </svg>

        <span class="dname">{{ row.entry.name }}</span>
      </div>
      <p v-if="rootEntries && rootEntries.length === 0" class="dnone">
        没有子目录或 PDF 文件
      </p>
    </div>
  </aside>
</template>

<style scoped>
.dtree {
  display: flex;
  flex-direction: column;
  min-width: 0;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  background: #fff;
  overflow: hidden;
}
.dhead {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  border-bottom: 1px solid #d0d7de;
  background: #f6f8fa;
}
.dtitle {
  font-weight: 600;
  margin-right: 4px;
  white-space: nowrap;
}
.dspacer {
  flex: 1;
}
.dhead button {
  padding: 2px 8px;
  font-size: 12px;
  border: 1px solid #d0d7de;
  border-radius: 5px;
  background: #fff;
  cursor: pointer;
  white-space: nowrap;
}
.dhead button:disabled {
  opacity: 0.5;
  cursor: default;
}
.dhead button:not(:disabled):hover {
  background: #f0f6ff;
  border-color: #1f6feb;
}
.droot {
  margin: 0;
  padding: 4px 10px;
  font-size: 12px;
  color: #57606a;
  border-bottom: 1px solid #eaeef2;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.derr {
  margin: 0;
  padding: 4px 10px;
  font-size: 12px;
  color: #cf222e;
}
.dempty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  color: #57606a;
  text-align: center;
  margin: 0;
}
.dempty button {
  padding: 5px 14px;
  border: 1px solid #1f6feb;
  border-radius: 6px;
  background: #1f6feb;
  color: #fff;
  cursor: pointer;
}
.dbody {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 4px 0;
}
.drow {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding-right: 8px;
  cursor: default;
  user-select: none;
  white-space: nowrap;
}
.drow:hover {
  background: #f0f6ff;
}
.drow.sel {
  background: #dbeafe;
}
.twisty {
  flex: none;
  width: 14px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #57606a;
  cursor: pointer;
}
.twisty svg {
  transition: transform 0.12s;
}
.twisty.open svg {
  transform: rotate(90deg);
}
.twisty.off {
  visibility: hidden;
  pointer-events: none;
}
.dicon {
  flex: none;
}
.dname {
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}
.dnone {
  padding: 12px;
  color: #8b949e;
  font-size: 12px;
  text-align: center;
  margin: 0;
}
.spin {
  width: 10px;
  height: 10px;
  border: 2px solid #d0d7de;
  border-top-color: #1f6feb;
  border-radius: 50%;
  animation: dtspin 0.7s linear infinite;
}
@keyframes dtspin {
  to {
    transform: rotate(360deg);
  }
}
</style>
