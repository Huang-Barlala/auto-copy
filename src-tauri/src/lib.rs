use anyhow::anyhow;
use log::{error, info, warn};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder},
    Manager,
};
use tauri_plugin_log::{Target, TargetKind};
use tokio::{
    spawn,
    sync::{
        mpsc::{channel, Receiver},
        Mutex,
    },
};

mod file_processor;
mod file_processor_copy;
mod file_processor_link;

struct AppState {
    watcher: Mutex<HashMap<String, RecommendedWatcher>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum CopyType {
    Copy,
    Link,
}

#[tauri::command]
async fn watch(
    id: String,
    from: &str,
    to: &str,
    copy_type: CopyType,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut watcher_guard = state.watcher.lock().await;
    if watcher_guard.contains_key(&id) {
        return Err(format!("路径 '{}' 已在监视中。", from));
    }
    // 为新路径创建并启动监视器
    match start_watching_path(from, to, copy_type).await {
        Ok(wather) => {
            watcher_guard.insert(id, wather);
            Ok(())
        }
        Err(e) => Err(format!("启动对路径 '{}' 的监视失败: {}", from, e)),
    }
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

async fn start_watching_path(
    from: &str,
    to: &str,
    copy_type: CopyType,
) -> anyhow::Result<RecommendedWatcher> {
    let from_path = PathBuf::from(from);
    let to_path = PathBuf::from(to);

    // 检查源路径是否存在
    if !from_path.exists() {
        return Err(anyhow!("源路径 '{}' 不存在。请确保路径正确。", from));
    }
    // 检查目标路径是否存在
    if !to_path.exists() {
        return Err(anyhow!("目标路径 '{}' 不存在。请确保路径正确。", to));
    }

    let (mut watcher, mut rx) = setup_watcher_channel()?;

    // 尝试监视路径。如果失败，错误将被传播。
    watcher.watch(&from_path, RecursiveMode::Recursive)?;
    // 生成一个新任务来处理事件。
    // `watcher` 实例被移动到此任务中以使其保持活动状态。
    spawn(async move {
        info!("路径 {:?} 的事件处理循环已启动。", from_path);
        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => file_processor::process(copy_type, event, &from_path, &to_path).await,
                Err(e) => error!("监视路径 {:?} 时出错: {:?}", from_path, e),
            }
        }
        info!("路径 {:?} 的事件处理循环已停止。", from_path);
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
                    Target::new(TargetKind::Folder {
                        path: "./logs".into(),
                        file_name: Some("tauri".into()),
                    }),
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
        .setup(|app| {
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_i])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {
                        warn!("menu item {:?} not handled", event.id);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![watch, stop_watching])
        .build(tauri::generate_context!())
        .expect("运行Tauri应用程序时出错")
        .run(|app, event| {
            if let tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } = event
            {
                let window = app.get_webview_window(&label).expect("获取窗口失败");
                window.hide().expect("隐藏窗口失败");
                api.prevent_close();
            }
        });
}
