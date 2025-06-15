use std::path::{self};

use log::{error, info, warn};

use crate::{file_processor_copy::CopyProcessor, file_processor_link::LinkProcessor, CopyType};

pub trait FileProcessor {
    fn deal_create(
        kind: notify::event::CreateKind,
        path: &path::Path,
        from: &path::Path,
        to: &path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
    fn deal_modify(
        kind: notify::event::ModifyKind,
        path: &path::Path,
        from: &path::Path,
        to: &path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
    fn deal_remove(
        kind: notify::event::RemoveKind,
        path: &path::Path,
        from: &path::Path,
        to: &path::Path,
    ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send;
}

pub async fn process(
    copy_type: CopyType,
    event: notify::Event,
    from: &path::Path,
    to: &path::Path,
) {
    if let Some(path) = event.paths.first() {
        match event.kind {
            notify::EventKind::Create(create_kind) => {
                if let Err(e) = match copy_type {
                    CopyType::Copy => CopyProcessor::deal_create(create_kind, path, from, to).await,
                    CopyType::Link => LinkProcessor::deal_create(create_kind, path, from, to).await,
                } {
                    error!("处理创建事件时出错: {:?}", e);
                }
            }
            notify::EventKind::Modify(modify_kind) => {
                if let Err(e) = match copy_type {
                    CopyType::Copy => CopyProcessor::deal_modify(modify_kind, path, from, to).await,
                    CopyType::Link => LinkProcessor::deal_modify(modify_kind, path, from, to).await,
                } {
                    error!("处理修改事件时出错: {:?}", e);
                }
            }
            notify::EventKind::Remove(remove_kind) => {
                if let Err(e) = match copy_type {
                    CopyType::Copy => CopyProcessor::deal_remove(remove_kind, path, from, to).await,
                    CopyType::Link => LinkProcessor::deal_remove(remove_kind, path, from, to).await,
                } {
                    error!("处理删除事件时出错: {:?}", e);
                }
            }
            default => {
                info!("未处理的事件类型: {:?}", default);
            }
        }
    } else {
        warn!("事件没有路径信息: {:?}", event);
    }
}

pub async fn delete(path: &path::Path) -> anyhow::Result<()> {
    if path.is_symlink() {
        std::fs::remove_file(path)?;
        info!("已成功删除软链接: {:?}", path);
    } else if path.is_file() {
        std::fs::remove_file(path)?;
        info!("已成功删除文件: {:?}", path);
    } else if path.is_dir() {
        std::fs::remove_dir_all(path)?;
        info!("已成功删除目录: {:?}", path);
    } else {
        return Err(anyhow::anyhow!(
            "无法删除: {:?}, 不是文件、目录或软链接",
            path
        ));
    }
    Ok(())
}
