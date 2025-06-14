use log::{error, info};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tauri_plugin_log::{Target, TargetKind};
use tokio::{
    spawn,
    sync::{
        mpsc::{channel, Receiver},
        Mutex,
    },
};

struct AppState {
    watcher: Mutex<HashMap<String, RecommendedWatcher>>,
}

#[derive(Serialize, Deserialize, Debug)]
enum CopyType {
    Copy,
    Link,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("你好, {}! 你收到了来自Rust的问候!", name)
}

#[tauri::command]
async fn watch(
    id: String,
    path: &str,
    copy_type: CopyType,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut watcher_guard = state.watcher.lock().await;
    if watcher_guard.contains_key(&id) {
        return Err(format!("路径 '{}' 已在监视中。", path));
    }
    // 为新路径创建并启动监视器
    match start_watching_path(path).await {
        Ok(wather) => {
            watcher_guard.insert(id, wather);
            Ok(())
        }
        Err(e) => Err(format!("启动对路径 '{}' 的监视失败: {}", path, e)),
    }
}

fn setup_watcher_channel() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)>
{
    let (tx, rx) = channel(200);
    let watcher = RecommendedWatcher::new(
        move |res| {
            if tx.try_send(res).is_err() {
                error!("发送事件错误: 通道可能已满或已关闭。");
            }
        },
        notify::Config::default(),
    )?;

    Ok((watcher, rx))
}

#[tauri::command]
async fn stop_watching(id: &str, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut watcher_guard = state.watcher.lock().await;
    if watcher_guard.remove(id).is_some() {
        // 当 RecommendedWatcher 从 HashMap 中移除并被 drop 时，
        // 其内部的发送端 (tx) 会被 drop，导致接收端 (rx) 的 recv() 方法返回 None，
        // 从而使关联的异步任务优雅地停止。
        // notify crate 的 RecommendedWatcher 在 Drop 时也会清理其监视的路径。
        info!("已成功停止对id '{}' 的监视。", id);
        Ok(())
    } else {
        Err(format!("id '{}' 未在监视中或无法停止。", id))
    }
}
async fn start_watching_path(path_str: &str) -> notify::Result<RecommendedWatcher> {
    let path_to_watch = PathBuf::from(path_str);

    let (mut watcher, mut rx) = setup_watcher_channel()?;

    // 尝试监视路径。如果失败，错误将被传播。
    watcher.watch(&path_to_watch, RecursiveMode::Recursive)?;
    // 生成一个新任务来处理事件。
    // `watcher` 实例被移动到此任务中以使其保持活动状态。
    spawn(async move {
        info!("路径 {:?} 的事件处理循环已启动。", path_to_watch);
        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => info!("检测到变更: {:?} 位于路径 {:?}", event.kind, event.paths),
                Err(e) => error!("监视路径 {:?} 时出错: {:?}", path_to_watch, e),
            }
        }
        info!("路径 {:?} 的事件处理循环已停止。", path_to_watch);
        // watcher 在任务作用域结束时隐式删除。
    });

    // 返回 Ok，表示监视器已成功初始化并且事件循环已生成。
    Ok(watcher)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .manage(AppState {
            watcher: Default::default(),
        })
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, watch, stop_watching])
        .run(tauri::generate_context!())
        .expect("运行Tauri应用程序时出错");
}
