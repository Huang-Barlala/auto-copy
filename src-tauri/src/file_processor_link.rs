use anyhow::anyhow;
use log::{error, info};

use crate::file_processor::{self, FileProcessor};

pub struct LinkProcessor;

impl FileProcessor for LinkProcessor {
    async fn deal_create(
        kind: notify::event::CreateKind,
        path: &std::path::Path,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> anyhow::Result<()> {
        match kind {
            notify::event::CreateKind::Any
            | notify::event::CreateKind::File
            | notify::event::CreateKind::Folder => create_link(path, from, to).await,

            notify::event::CreateKind::Other => {
                // 处理其他类型的创建
                info!("创建了其他类型的文件或目录: {:?}", path);
                Ok(())
            }
        }
    }

    async fn deal_modify(
        kind: notify::event::ModifyKind,
        path: &std::path::Path,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> anyhow::Result<()> {
        match kind {
            notify::event::ModifyKind::Name(rename_mode) => {
                info!("文件名被修改: {:?}, {:?}", rename_mode, path);
                match rename_mode {
                    notify::event::RenameMode::To => create_link(path, from, to).await,
                    notify::event::RenameMode::From => delete(path, from, to).await,
                    _ => {
                        info!("未处理的重命名模式: {:?}", rename_mode);
                        Ok(())
                    }
                }
            }
            _ => {
                info!("修改事件未处理: {:?}", kind);
                Ok(())
            }
        }
    }

    async fn deal_remove(
        _kind: notify::event::RemoveKind,
        path: &std::path::Path,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> anyhow::Result<()> {
        delete(path, from, to).await
    }
}

async fn create_link(
    path: &std::path::Path,
    from: &std::path::Path,
    to: &std::path::Path,
) -> anyhow::Result<()> {
    if path.is_dir() {
        info!("忽略目录链接: {:?}", path);
        return Ok(());
    }

    let relative_path = path.strip_prefix(from)?;
    let target_path = to.join(relative_path);
    // 确保目标路径的父目录存在
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent).await?;
        }
    } else {
        return Err(anyhow!("无法获取目标路径的父目录: {:?}", target_path));
    }

    // 检查目标路径是否已存在链接，不相同则删除
    if target_path.is_symlink() {
        match tokio::fs::read_link(&target_path).await {
            Ok(old_link) => {
                if old_link == path {
                    info!("链接已存在且指向相同的路径: {:?}", target_path);
                    return Ok(());
                } else {
                    tokio::fs::remove_file(&target_path).await?;
                    info!("已删除旧链接: {:?}", target_path);
                }
            }
            Err(e) => {
                error!("读取链接失败: {:?}", e);
                tokio::fs::remove_file(&target_path).await?;
                info!("已删除旧链接: {:?}", target_path);
            }
        }
    }
    // 删除已存在普通文件
    else if target_path.is_file() {
        tokio::fs::remove_file(&target_path).await?;
        info!("已删除旧文件: {:?}", target_path);
    }

    tokio::fs::symlink_file(path, &target_path).await?;
    info!("已成功创建文件链接: {:?} 到 {:?}", path, target_path);

    Ok(())
}

async fn delete(
    path: &std::path::Path,
    from: &std::path::Path,
    to: &std::path::Path,
) -> anyhow::Result<()> {
    let relative_path = path.strip_prefix(from)?;
    let target_path = to.join(relative_path);
    file_processor::delete(&target_path).await
}
