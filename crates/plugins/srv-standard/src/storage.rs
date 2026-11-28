//! 服务端文件持久化：内容寻址块落盘 + VFS 索引落盘 + 14 天 GC。
//!
//! - 块内容按 SHA256 hex 内容寻址存于 `chunks/<hash>`；上传即写盘。
//! - VFS 树索引序列化到 `index.yaml`；每次变更后增量落盘。
//! - GC 按 `modified_at`（最后修改时间）判定，超 14 天的文件条目移除，
//!   未被任何存活条目引用的孤立块一并清理；不依赖跨平台不可靠的 atime。
//! - 数据根默认 `~/.terminal_chat/server/`，可用 `TC_SRV_DATA_DIR` 覆盖。

use crate::{VFS, VfsUser, now_ts};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// GC 回收阈值：14 天（秒）。
pub(crate) const GC_TTL_SECS: u64 = 14 * 24 * 3600;
/// 后台 GC 心跳间隔：24 小时检查一次是否到 GC 时点。
const HEARTBEAT_SECS: u64 = 24 * 3600;
/// 插件卸载时置位，通知 GC 心跳线程退出。
static GC_STOP: AtomicBool = AtomicBool::new(false);
const DIR_NAME: &str = "server";
const CHUNKS_DIR: &str = "chunks";
const INDEX_FILE: &str = "index.yaml";

/// 落盘索引结构：freq -> VFS 用户视图 + 上次 GC 时间戳。
#[derive(Serialize, Deserialize, Default)]
struct IndexFile {
    #[serde(default)]
    freqs: HashMap<String, VfsUser>,
    #[serde(default)]
    last_gc: u64,
}

/// 数据根目录（创建缺失目录）。
pub(crate) fn data_root() -> PathBuf {
    let root = match std::env::var("TC_SRV_DATA_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".terminal_chat").join(DIR_NAME)
        }
    };
    let _ = fs::create_dir_all(&root);
    root
}

fn chunks_dir() -> PathBuf {
    let dir = data_root().join(CHUNKS_DIR);
    let _ = fs::create_dir_all(&dir);
    dir
}

fn index_path() -> PathBuf {
    data_root().join(INDEX_FILE)
}

/// 校验分片 hash（SHA256 hex）并按 hash 把块内容写入内容寻址存储；
/// hash 不符返回错误。已存在的同名块跳过写入（内容寻址去重）。
pub(crate) fn store_chunk(hash: &str, data: &[u8]) -> Result<(), String> {
    let digest = format!("{:x}", Sha256::digest(data));
    if digest != hash {
        return Err("chunk hash mismatch".to_string());
    }
    let path = chunks_dir().join(hash);
    if !path.exists() {
        fs::write(&path, data).map_err(|e| format!("write chunk failed: {e}"))?;
    }
    Ok(())
}

/// 按 hash 读取一个块；缺失返回 `None`。
pub(crate) fn read_chunk(hash: &str) -> Option<Vec<u8>> {
    fs::read(chunks_dir().join(hash)).ok()
}

/// 按块 hash 列表重组文件内容；缺失块跳过（与原内存实现同等容错）。
pub(crate) fn reassemble(chunks: &[String]) -> Vec<u8> {
    let mut out = Vec::with_capacity(chunks.len());
    for h in chunks {
        if let Some(data) = read_chunk(h) {
            out.extend_from_slice(&data);
        }
    }
    out
}

/// 把当前 VFS 树与上次 GC 时间戳原子写入索引（临时文件 + rename）。
pub(crate) fn save_index(vfs: &HashMap<String, VfsUser>, last_gc: u64) {
    let idx = IndexFile { freqs: vfs.clone(), last_gc };
    let Ok(bytes) = serde_yaml::to_string(&idx) else {
        return;
    };
    let tmp = index_path().with_extension("yaml.tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, index_path());
    }
}

/// 从索引文件加载 VFS 树与上次 GC 时间戳；文件缺失或解析失败返回空。
pub(crate) fn load_index() -> (HashMap<String, VfsUser>, u64) {
    let Ok(text) = fs::read_to_string(index_path()) else {
        return (HashMap::new(), 0);
    };
    match serde_yaml::from_str::<IndexFile>(&text) {
        Ok(idx) => (idx.freqs, idx.last_gc),
        Err(_) => (HashMap::new(), 0),
    }
}

/// 若距上次 GC 已达阈值，回收超期文件条目与孤立块，并更新 `last_gc`。
pub(crate) fn gc_if_due(now: u64) {
    let (_, last_gc) = load_index();
    if now.saturating_sub(last_gc) < GC_TTL_SECS {
        return;
    }

    let mut live: HashSet<String> = HashSet::new();
    {
        let mut vfs = VFS.lock().unwrap();
        for v in vfs.values_mut() {
            // 移除超期普通文件；存活文件引用的块记入 live 集合。
            v.files.retain(|_, e| {
                if e.is_dir {
                    return true;
                }
                if now.saturating_sub(e.modified_at) < GC_TTL_SECS {
                    live.extend(e.chunks.iter().cloned());
                    true
                } else {
                    false
                }
            });
            // 先快照仍有子项的目录，再移除其余空目录（避免闭包内重复借用 files）。
            let dirs_with_children: HashSet<String> = v
                .files
                .iter()
                .filter(|(_, e)| e.is_dir)
                .filter(|(path, _)| {
                    let prefix = format!("{}/", path.trim_end_matches('/'));
                    v.files.iter().any(|(p, _)| p != *path && p.starts_with(&prefix))
                })
                .map(|(path, _)| path.clone())
                .collect();
            v.files.retain(|path, e| !e.is_dir || dirs_with_children.contains(path));
        }
        vfs.retain(|_, v| !v.files.is_empty());
        save_index(&vfs, now);
    }

    // 锁外清理未被存活条目引用的孤立块。
    if let Ok(rd) = fs::read_dir(chunks_dir()) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if !live.contains(&name) {
                let _ = fs::remove_file(ent.path());
            }
        }
    }
}

/// 启动后台 GC 心跳线程：每 24 小时检查一次，到达 14 天阈值即回收。
pub(crate) fn spawn_gc_heartbeat() {
    GC_STOP.store(false, Ordering::Relaxed);
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(HEARTBEAT_SECS));
            if GC_STOP.load(Ordering::Relaxed) {
                return;
            }
            gc_if_due(now_ts());
        }
    });
}

/// 通知 GC 心跳线程退出（插件卸载时调用）。
pub(crate) fn stop_gc_heartbeat() {
    GC_STOP.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VfsEntry;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tc_srv_storage_{tag}_{}", now_ts()));
        let _ = fs::create_dir_all(&dir);
        unsafe { std::env::set_var("TC_SRV_DATA_DIR", &dir) };
        dir
    }

    #[test]
    fn store_read_reassemble_round_trip() {
        let _dir = temp_root("roundtrip");
        let data = b"hello terminal chat";
        let hash = format!("{:x}", Sha256::digest(data.as_slice()));
        store_chunk(&hash, data).unwrap();
        let body = reassemble(&[hash]);
        assert_eq!(body, data);
    }

    #[test]
    fn store_chunk_rejects_mismatched_hash() {
        let _dir = temp_root("mismatch");
        assert!(store_chunk("deadbeef", b"data").is_err());
    }

    #[test]
    fn index_save_load_round_trip() {
        let _dir = temp_root("index");
        let mut files = HashMap::new();
        files.insert(
            "/a/b.txt".to_string(),
            VfsEntry { is_dir: false, size: 18, chunks: vec!["abc".to_string()], modified_at: 100 },
        );
        let mut vfs = HashMap::new();
        vfs.insert("0.5".to_string(), VfsUser { cwd: "/".to_string(), files });
        save_index(&vfs, 42);
        let (loaded, last_gc) = load_index();
        assert_eq!(last_gc, 42);
        let entry = &loaded["0.5"].files["/a/b.txt"];
        assert_eq!(entry.size, 18);
        assert_eq!(entry.chunks, vec!["abc".to_string()]);
    }

    #[test]
    fn gc_reclaims_expired_entries_and_orphan_chunks() {
        let _dir = temp_root("gc");
        let now = now_ts();
        let alive_data = b"alive block";
        let expired_data = b"expired block";
        let alive_hash = format!("{:x}", Sha256::digest(alive_data));
        let expired_hash = format!("{:x}", Sha256::digest(expired_data));
        store_chunk(&alive_hash, alive_data).unwrap();
        store_chunk(&expired_hash, expired_data).unwrap();

        let mut files = HashMap::new();
        files.insert(
            "/alive.txt".to_string(),
            VfsEntry {
                is_dir: false,
                size: alive_data.len() as u64,
                chunks: vec![alive_hash.clone()],
                modified_at: now,
            },
        );
        files.insert(
            "/expired.txt".to_string(),
            VfsEntry {
                is_dir: false,
                size: expired_data.len() as u64,
                chunks: vec![expired_hash.clone()],
                modified_at: now.saturating_sub(30 * 86400),
            },
        );
        let mut vfs = HashMap::new();
        vfs.insert("0.5".to_string(), VfsUser { cwd: "/".to_string(), files });
        {
            let mut g = crate::VFS.lock().unwrap();
            *g = vfs;
        }

        gc_if_due(now);

        let after = crate::VFS.lock().unwrap();
        let room = &after["0.5"];
        assert!(room.files.contains_key("/alive.txt"));
        assert!(!room.files.contains_key("/expired.txt"));
        assert!(read_chunk(&alive_hash).is_some());
        assert!(read_chunk(&expired_hash).is_none());
    }
}
