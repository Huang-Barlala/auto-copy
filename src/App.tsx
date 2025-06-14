import { createEffect, createSignal, For, on, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { createStore } from "solid-js/store";
import { Store } from "@tauri-apps/plugin-store";
import debounce from "debounce";
import { trackStore } from "@solid-primitives/deep";
import { v7 as uuidv7 } from "uuid";
import { attachConsole } from "@tauri-apps/plugin-log";

type CopyConfiguration = {
  id: string;
  from: string;
  to: string;
  type: "Copy" | "Link";
  enable: boolean;
  error?: string;
};
type ToastMessage = {
  id: number;
  message: string;
  type: "success" | "error" | "info" | "warning";
};

const detach = await attachConsole();
const store = await Store.load("store.json");
detach();

function App() {
  // --- 状态管理 ---
  // 使用 Solid 的 createStore 来响应式管理配置状态
  const [copyConfs, setCopyConfs] = createStore<CopyConfiguration[]>([]);
  // 用于控制删除确认弹框的状态，null 表示隐藏
  const [deletingIndex, setDeletingIndex] = createSignal<number | null>(null);
  // Toast 消息状态
  const [toasts, setToasts] = createSignal<ToastMessage[]>([]);
  let toastIdCounter = 0;

  // --- 组件生命周期: onMount ---
  // 组件首次加载时，从 store 加载现有配置
  onMount(async () => {
    try {
      const oldConf = await store.get<CopyConfiguration[]>("copyConfs");
      if (oldConf && Array.isArray(oldConf)) {
        setCopyConfs(oldConf);
      }
    } catch (error) {
      console.error("从 store 加载配置失败:", error);
    }
  });

  // --- Toast 管理函数 ---
  function showToast(
    message: string,
    type: ToastMessage["type"] = "info",
    duration = 5000
  ) {
    const id = toastIdCounter++;
    setToasts((prev) => [...prev, { id, message, type }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, duration);
  }

  // --- 防抖保存函数 ---
  // 为避免每次按键或点击都写入磁盘，我们对保存操作进行防抖处理。
  // 它只会在1秒无操作后执行。
  const debouncedSave = debounce(async (data: CopyConfiguration[]) => {
    try {
      // 从 Solid 的代理 store 创建一个纯 JS 数组
      await store.set("copyConfs", data);
      await store.save(); // 显式地将 store 保存到磁盘
      console.log("配置已保存。");
    } catch (error) {
      console.error("保存配置失败:", error);
    }
  }, 1000);

  // --- 自动保存 Effect ---
  // 此 effect 会深度跟踪 copyConfs 对象数组中的变化。
  // 当检测到变化时，它会触发防抖保存函数。
  createEffect(
    on(
      () => trackStore(copyConfs),
      (confs) => {
        debouncedSave(confs);
      }
    )
  );

  // --- UI 事件处理器 ---

  /**
   * 向列表中添加一个新的空配置对象
   */
  function addNewConf() {
    setCopyConfs(copyConfs.length, {
      id: uuidv7(),
      from: "",
      to: "",
      type: "Copy",
      enable: false,
    });
  }

  /**
   * 打开原生目录选择对话框并更新路径
   * @param {number} index - 要更新的配置索引
   * @param {'from' | 'to'} type - 要更新的路径类型 ('from' 或 'to')
   */
  async function selectDir(index: number, type: "from" | "to") {
    try {
      const dir = await open({
        multiple: false,
        directory: true,
        title: `选择 '${type}' 目录`,
      });
      // 如果用户取消对话框，`open` 返回 null
      if (dir !== null && typeof dir === "string") {
        setCopyConfs(index, type, dir);
      }
    } catch (error) {
      console.error("打开目录对话框时出错:", error);
    }
  }

  /**
   * 根据索引从列表中移除一个配置
   * @param {number} index - 要删除的配置索引
   */
  function deleteConf(index: number) {
    // 重新创建不含指定索引的数组
    setCopyConfs((prev) => prev.filter((_, i) => i !== index));
  }

  /**
   * 处理删除确认操作
   */
  function handleConfirmDelete() {
    const index = deletingIndex();
    if (index !== null) {
      deleteConf(index);
    }
    setDeletingIndex(null); // 关闭弹框
    showToast("配置已删除", "success");
  }

  /**
   * 切换配置的 'enable' 状态
   * @param {number} index - 要切换的配置索引
   */
  function toggleEnable(index: number) {
    setCopyConfs(index, "enable", (prev) => !prev);

    if (copyConfs[index].enable) {
      invoke("watch", {
        id: copyConfs[index].id,
        path: copyConfs[index].from,
        copyType: copyConfs[index].type,
      });
    } else {
      invoke("stop_watching", {
        id: copyConfs[index].id,
      });
    }
  }
  return (
    <div class="bg-base-200 min-h-screen p-4 sm:p-8">
      <main class="max-w-4xl mx-auto">
        <div class="text-center mb-8">
          <h1 class="text-4xl font-bold mb-2">同步配置</h1>
          <p class="text-base-content/70">在这里管理您的目录同步与链接设置</p>
        </div>

        <div class="space-y-4">
          <For each={copyConfs}>
            {(conf, index) => (
              <div class="card w-full bg-base-100 shadow-lg transition-all hover:shadow-xl">
                <div class="card-body p-4 sm:p-6">
                  <div class="flex flex-col md:flex-row gap-4 items-center">
                    {/* --- 路径输入 --- */}
                    <div class="w-full flex-grow space-y-3">
                      <label
                        class="input input-bordered flex items-center gap-3 cursor-pointer w-full"
                        onClick={() => {
                          if (conf.enable) {
                            showToast("启动时无法修改配置", "warning");
                          } else {
                            selectDir(index(), "from");
                          }
                        }}
                      >
                        <span class="badge badge-info">源</span>
                        <input
                          readonly
                          type="text"
                          class="grow cursor-pointer"
                          placeholder="点击选择源目录"
                          value={conf.from}
                        />
                      </label>
                      <label
                        class="input input-bordered flex items-center gap-3 cursor-pointer w-full"
                        onClick={() => {
                          if (conf.enable) {
                            showToast("启动时无法修改配置", "warning");
                          } else {
                            selectDir(index(), "to");
                          }
                        }}
                      >
                        <span class="badge badge-success">目标</span>
                        <input
                          readonly
                          type="text"
                          class="grow cursor-pointer"
                          placeholder="点击选择目标目录"
                          value={conf.to}
                        />
                      </label>
                      {/* --- 模式选择器 --- */}
                      <div class="join w-full">
                        <button
                          class="btn join-item flex-grow"
                          classList={{
                            "btn-primary": conf.type === "Copy",
                            "btn-active": conf.type === "Copy" && !conf.enable,
                          }}
                          onClick={() => {
                            if (conf.enable) {
                              showToast("启动时无法修改配置", "warning");
                            } else {
                              setCopyConfs(index(), "type", "Copy");
                            }
                          }}
                        >
                          复制模式
                        </button>
                        <button
                          class="btn join-item flex-grow"
                          classList={{
                            "btn-primary": conf.type === "Link",
                            "btn-active": conf.type === "Link" && !conf.enable,
                          }}
                          onClick={() => {
                            if (conf.enable) {
                              showToast("启动时无法修改配置", "warning");
                            } else {
                              setCopyConfs(index(), "type", "Link");
                            }
                          }}
                        >
                          链接模式
                        </button>
                      </div>
                    </div>

                    {/* --- 控制器 --- */}
                    <div class="flex items-center gap-4 self-center md:self-auto pt-2 md:pt-0">
                      <input
                        type="checkbox"
                        class="toggle toggle-lg toggle-success"
                        checked={conf.enable}
                        title={conf.enable ? "禁用" : "启用"}
                        onChange={() => toggleEnable(index())}
                      />
                      <div
                        class="tooltip tooltip-left"
                        data-tip={
                          conf.error
                            ? conf.error
                            : conf.enable
                            ? "已启用"
                            : "已禁用"
                        }
                      >
                        <div class="relative w-4 h-4 flex items-center justify-center">
                          {/* 波纹动画效果 - 仅在启用时渲染 */}
                          <Show when={conf.enable && !conf.error}>
                            <div class="absolute inline-flex h-full w-full rounded-full bg-success animate-ping [animation-duration:2s]"></div>
                          </Show>
                          {/* 静态可见圆点 */}
                          <div
                            class="relative inline-flex rounded-full h-4 w-4"
                            classList={{
                              "bg-success": conf.enable && !conf.error,
                              "bg-base-300": !conf.enable,
                              "bg-error": !!conf.error,
                            }}
                          ></div>
                        </div>
                      </div>
                      <button
                        class="btn btn-circle btn-ghost"
                        onClick={() => setDeletingIndex(index())}
                        title="删除此配置"
                      >
                        <TrashIcon />
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}
          </For>
        </div>

        <div class="text-center mt-8">
          <button class="btn btn-primary btn-wide" onClick={addNewConf}>
            添加新配置
          </button>
        </div>
      </main>
      {/* --- 删除确认弹框 --- */}
      <Show when={deletingIndex() !== null}>
        <div class="modal modal-open">
          <div class="modal-box">
            <h3 class="font-bold text-lg">确认删除</h3>
            <p class="py-4">您确定要删除这个同步配置吗？此操作无法撤销。</p>
            <div class="modal-action">
              <button class="btn" onClick={() => setDeletingIndex(null)}>
                取消
              </button>
              <button class="btn btn-error" onClick={handleConfirmDelete}>
                确认删除
              </button>
            </div>
          </div>
          {/* 点击背景关闭弹窗 */}
          <form method="dialog" class="modal-backdrop">
            <button onClick={() => setDeletingIndex(null)}>close</button>
          </form>
        </div>
      </Show>
      {/* --- Toast 容器 --- */}
      <div class="toast toast-end toast-bottom z-50 p-4">
        <For each={toasts()}>
          {(toastItem) => (
            <div
              class={`alert shadow-lg mt-2`}
              classList={{
                "alert-warning": toastItem.type === "warning",
                "alert-error": toastItem.type === "error",
                "alert-success": toastItem.type === "success",
                "alert-info": toastItem.type === "info",
              }}
            >
              <span>{toastItem.message}</span>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

export default App;
const TrashIcon = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    class="h-6 w-6"
    fill="none"
    viewBox="0 0 24 24"
    stroke="currentColor"
  >
    <path
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-width="2"
      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
    />
  </svg>
);
