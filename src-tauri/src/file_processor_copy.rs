use std::path;

use anyhow::anyhow;
use log::info;

use crate::file_processor::{self, FileProcessor};

pub struct CopyProcessor;
impl FileProcessor for CopyProcessor {
    async fn deal_create(
        kind: notify::event::CreateKind,
        path: &std::path::Path,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> anyhow::Result<()> {
        match kind {
            notify::event::CreateKind::File => copy(path, from, to).await,
            notify::event::CreateKind::Folder => {
                info!("无处理，创建了一个目录: {:?}", path);
                Ok(())
            }
            notify::event::CreateKind::Other => {
                info!("无处理，创建了其他类型的文件或目录:  {:?}", path);
                Ok(())
            }
            notify::event::CreateKind::Any => {
                if path.is_file() {
                    copy(path, from, to).await
                } else {
                    info!("无处理，创建了一个目录: {:?}", path);
                    Ok(())
                }
            }
        }
    }

    async fn deal_modify(
        kind: notify::event::ModifyKind,
        path: &path::Path,
        from: &path::Path,
        to: &path::Path,
    ) -> anyhow::Result<()> {
        match kind {
            notify::event::ModifyKind::Data(data) => {
                info!("数据被修改: {:?}, {:?}", data, path);
                copy(path, from, to).await
            }
            notify::event::ModifyKind::Metadata(metadata) => {
                info!("元数据被修改: {:?}, {:?}", metadata, path);
                Ok(())
            }
            notify::event::ModifyKind::Name(rename) => {
                info!("文件名被修改: {:?}, {:?}", rename, path);
                match rename {
                    notify::event::RenameMode::To => copy(path, from, to).await,
                    notify::event::RenameMode::From => delete(path, from, to).await,
                    default => {
                        info!("未处理的重命名模式: {:?}", default);
                        Ok(())
                    }
                }
            }
            notify::event::ModifyKind::Other => {
                info!("其他类型的修改: {:?}", path);
                Ok(())
            }
            notify::event::ModifyKind::Any => {
                info!("任意类型的修改: {:?}", path);
                copy(path, from, to).await
            }
        }
    }

    async fn deal_remove(
        _kind: notify::event::RemoveKind,
        path: &path::Path,
        from: &path::Path,
        to: &path::Path,
    ) -> anyhow::Result<()> {
        delete(path, from, to).await
    }
}

async fn copy(path: &path::Path, from: &path::Path, to: &path::Path) -> anyhow::Result<()> {
    let relative_path = path.strip_prefix(from)?;
    let target_path = to.join(relative_path);
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent).await?;
        }
    } else {
        return Err(anyhow!("无法获取目标路径的父目录: {:?}", target_path));
    }
    tokio::fs::copy(path, &target_path).await?;
    info!("已成功复制文件: {:?} 到 {:?}", path, target_path);
    Ok(())
}
async fn delete(path: &path::Path, from: &path::Path, to: &path::Path) -> anyhow::Result<()> {
    let relative_path = path.strip_prefix(from)?;
    let target_path = to.join(relative_path);
    file_processor::delete(&target_path).await
}
