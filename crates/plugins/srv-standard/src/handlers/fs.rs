//! //! 远程文件系统（VFS 按 freq 共享）：pwd / ls / cd / mkdir / info / checksum / upload / download

#[allow(unused_imports)]
use crate::{
    FreqRoom, Member, NOTIFICATIONS, STATE, ServerState, UserProfile, VFS, VfsEntry, VfsUser,
    display_user, ensure_user, mod_auth, norm_path, notify, now_ts, user_freq, user_part,
    valid_freq, vfs_ensure_dirs,
};
use sha2::{Digest, Sha256};
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

pub(crate) fn handle_remote_pwd(
    sender: String,
    _req: protos::remote::fs::PwdRequest,
) -> protos::remote::fs::PwdResponse {
    use protos::remote::fs::PwdResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return PwdResponse {
                directory: String::new(),
                success: false,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    PwdResponse { directory: vfs.cwd.clone(), success: true, message: vfs.cwd.clone() }
}

pub(crate) fn handle_remote_ls(
    sender: String,
    req: protos::remote::fs::LsRequest,
) -> protos::remote::fs::LsResponse {
    use protos::remote::fs::{FileEntry, LsResponse};
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return LsResponse {
                directory: String::new(),
                entries: vec![],
                success: false,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = if req.dir.is_empty() { vfs.cwd.clone() } else { norm_path(&vfs.cwd, &req.dir) };
    if target != "/" {
        match vfs.files.get(&target) {
            Some(e) if !e.is_dir => {
                return LsResponse {
                    directory: target.clone(),
                    entries: vec![FileEntry {
                        name: target.rsplit('/').next().unwrap_or("").to_string(),
                        is_dir: false,
                        size: e.size as i64,
                        modified_at: e.modified_at as i64,
                    }],
                    success: true,
                    message: format!("{target} is a file"),
                };
            }
            None => {
                return LsResponse {
                    directory: target.clone(),
                    entries: vec![],
                    success: false,
                    message: format!("`{target}` not found"),
                };
            }
            _ => {}
        }
    }
    let mut entries = Vec::new();
    let prefix = if target == "/" { String::new() } else { target.clone() };
    for (path, entry) in &vfs.files {
        if !path.starts_with(&format!("{prefix}/")) && path != &target {
            continue;
        }
        let rest = &path[prefix.len()..];
        if rest.is_empty() || rest == "/" {
            continue;
        }
        let rest = rest.trim_start_matches('/');
        if rest.contains('/') {
            continue; // 只列直接子项
        }
        entries.push(FileEntry {
            name: rest.to_string(),
            is_dir: entry.is_dir,
            size: entry.size as i64,
            modified_at: entry.modified_at as i64,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    LsResponse {
        directory: target.clone(),
        entries,
        success: true,
        message: format!("listing {target}"),
    }
}

/// 远程 rm：删除文件；目录需 recursive 才会连子项一并删除。
pub(crate) fn handle_remote_rm(
    sender: String,
    req: protos::remote::fs::RemoteRmRequest,
) -> protos::remote::fs::RemoteRmResponse {
    use protos::remote::fs::RemoteRmResponse;
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteRmResponse { success: false, message: "not in any freq".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq)
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.path);
    if target == "/" {
        return RemoteRmResponse {
            success: false,
            message: "cannot remove the root directory".into(),
        };
    }
    match vfs.files.get(&target) {
        None => RemoteRmResponse { success: false, message: format!("`{target}` not found") },
        Some(e) if e.is_dir && !req.recursive => RemoteRmResponse {
            success: false,
            message: format!("`{target}` is a directory (use `-r` to remove recursively)"),
        },
        Some(e) if e.is_dir => {
            let prefix = format!("{}/", target.trim_end_matches('/'));
            vfs.files.retain(|k, _| k != &target && !k.starts_with(&prefix));
            RemoteRmResponse { success: true, message: format!("removed directory {target}") }
        }
        Some(_) => {
            vfs.files.remove(&target);
            RemoteRmResponse { success: true, message: format!("removed {target}") }
        }
    }
}

/// 远程 mv：移动/重命名（目录连带子项整体迁移）。
pub(crate) fn handle_remote_mv(
    sender: String,
    req: protos::remote::fs::RemoteMvRequest,
) -> protos::remote::fs::RemoteMvResponse {
    use protos::remote::fs::RemoteMvResponse;
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteMvResponse { success: false, message: "not in any freq".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq)
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let from = norm_path(&vfs.cwd, &req.from);
    let to = norm_path(&vfs.cwd, &req.to);
    if from == to {
        return RemoteMvResponse {
            success: false,
            message: "source and destination are the same".into(),
        };
    }
    if !vfs.files.contains_key(&from) {
        return RemoteMvResponse { success: false, message: format!("`{from}` not found") };
    }
    if vfs.files.contains_key(&to) {
        return RemoteMvResponse { success: false, message: format!("`{to}` already exists") };
    }
    // 收集源路径（含目录的子项）并迁移。
    let prefix = format!("{}/", from.trim_end_matches('/'));
    let moved: Vec<(String, VfsEntry)> = vfs
        .files
        .iter()
        .filter(|(k, _)| k == &&from || k.starts_with(&prefix))
        .map(|(k, e)| (k.clone(), (*e).clone()))
        .collect();
    let now = now_ts();
    for (k, mut e) in moved {
        let new_key = if k == from { to.clone() } else { format!("{}{}", to, &k[from.len()..]) };
        if e.is_dir {
            e.modified_at = now;
        }
        vfs.files.insert(new_key, e);
        vfs.files.remove(&k);
    }
    RemoteMvResponse { success: true, message: format!("moved {from} -> {to}") }
}

/// 远程 cp：复制（目录连带子项复制）。
pub(crate) fn handle_remote_cp(
    sender: String,
    req: protos::remote::fs::RemoteCpRequest,
) -> protos::remote::fs::RemoteCpResponse {
    use protos::remote::fs::RemoteCpResponse;
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteCpResponse { success: false, message: "not in any freq".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq)
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let from = norm_path(&vfs.cwd, &req.from);
    let to = norm_path(&vfs.cwd, &req.to);
    if from == to {
        return RemoteCpResponse {
            success: false,
            message: "source and destination are the same".into(),
        };
    }
    if !vfs.files.contains_key(&from) {
        return RemoteCpResponse { success: false, message: format!("`{from}` not found") };
    }
    if vfs.files.contains_key(&to) {
        return RemoteCpResponse { success: false, message: format!("`{to}` already exists") };
    }
    let prefix = format!("{}/", from.trim_end_matches('/'));
    let copied: Vec<(String, VfsEntry)> = vfs
        .files
        .iter()
        .filter(|(k, _)| k == &&from || k.starts_with(&prefix))
        .map(|(k, e)| (k.clone(), (*e).clone()))
        .collect();
    let mut count = 0usize;
    for (k, e) in copied {
        let new_key = if k == from { to.clone() } else { format!("{}{}", to, &k[from.len()..]) };
        vfs.files.insert(new_key, e);
        count += 1;
    }
    RemoteCpResponse { success: true, message: format!("copied {from} -> {to} ({count} entries)") }
}

/// 远程 tree：以缩进文本展示目录树。
pub(crate) fn handle_remote_tree(
    sender: String,
    req: protos::remote::fs::RemoteTreeRequest,
) -> protos::remote::fs::RemoteTreeResponse {
    use protos::remote::fs::RemoteTreeResponse;
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return RemoteTreeResponse {
                success: false,
                root: String::new(),
                text: "not in any freq".into(),
            };
        }
    };
    let all_vfs = VFS.lock().unwrap();
    let vfs = match all_vfs.get(&freq) {
        Some(v) => v,
        None => {
            return RemoteTreeResponse {
                success: false,
                root: String::new(),
                text: "empty directory".into(),
            };
        }
    };
    let target = if req.dir.is_empty() { vfs.cwd.clone() } else { norm_path(&vfs.cwd, &req.dir) };
    if target != "/" && !vfs.files.contains_key(&target) {
        return RemoteTreeResponse {
            success: false,
            root: target.clone(),
            text: format!("`{target}` not found"),
        };
    }
    let prefix =
        if target == "/" { String::new() } else { format!("{}/", target.trim_end_matches('/')) };
    let mut items: Vec<(String, bool)> = vfs
        .files
        .iter()
        .filter(|(k, _)| {
            *k != &target && (prefix.is_empty() && k.as_str() != "/" || k.starts_with(&prefix))
        })
        .map(|(k, e)| {
            let rel = if prefix.is_empty() {
                k.trim_start_matches('/').to_string()
            } else {
                k[prefix.len()..].to_string()
            };
            (rel, e.is_dir)
        })
        .filter(|(rel, _)| !rel.is_empty())
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut text = format!("{target}\n");
    for (rel, is_dir) in items {
        let depth = rel.matches('/').count();
        text.push_str(&format!(
            "{}{}{}\n",
            "  ".repeat(depth),
            rel.rsplit('/').next().unwrap_or(""),
            if is_dir { "/" } else { "" }
        ));
    }
    RemoteTreeResponse { success: true, root: target.clone(), text }
}

pub(crate) fn handle_remote_cd(
    sender: String,
    req: protos::remote::fs::CdRequest,
) -> protos::remote::fs::CdResponse {
    use protos::remote::fs::CdResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return CdResponse {
                directory: String::new(),
                success: false,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.dir);
    // 根目录 "/" 总是存在（无对应 VFS 条目也视为合法目录）。
    let exists = target == "/" || vfs.files.get(&target).map(|e| e.is_dir).unwrap_or(false);
    if !exists {
        return CdResponse {
            directory: vfs.cwd.clone(),
            success: false,
            message: format!("`{target}`: no such directory"),
        };
    }
    vfs.cwd = target.clone();
    CdResponse { directory: target.clone(), success: true, message: format!("now at {target}") }
}

pub(crate) fn handle_remote_mkdir(
    sender: String,
    req: protos::remote::fs::MkdirRequest,
) -> protos::remote::fs::MkdirResponse {
    use protos::remote::fs::MkdirResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return MkdirResponse {
                success: false,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let mut vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.directory);
    if vfs.files.contains_key(&target) {
        return MkdirResponse { success: false, message: format!("`{target}` already exists") };
    }
    let now = now_ts();
    vfs_ensure_dirs(&mut vfs, &target, now);
    MkdirResponse { success: true, message: format!("created directory {target}") }
}

pub(crate) fn handle_file_info(
    sender: String,
    req: protos::remote::fs::RemoteFileInfoRequest,
) -> protos::remote::fs::RemoteFileInfoResponse {
    use protos::remote::fs::{RemoteFileInfo, RemoteFileInfoResponse};
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return RemoteFileInfoResponse {
                found: false,
                info: None,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.file);
    match vfs.files.get(&target) {
        Some(e) => RemoteFileInfoResponse {
            found: true,
            info: Some(RemoteFileInfo {
                path: target.clone(),
                size: e.size as i64,
                modified_at: e.modified_at as i64,
                is_dir: e.is_dir,
            }),
            message: target.clone(),
        },
        None => RemoteFileInfoResponse {
            found: false,
            info: None,
            message: format!("`{target}` not found"),
        },
    }
}

pub(crate) fn handle_remote_checksum(
    sender: String,
    req: protos::remote::fs::RemoteChecksumRequest,
) -> protos::remote::fs::RemoteChecksumResponse {
    use protos::remote::fs::RemoteChecksumResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return RemoteChecksumResponse {
                checksum: String::new(),
                found: false,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.file);
    match vfs.files.get(&target) {
        Some(e) if !e.is_dir => {
            let digest = Sha256::digest(&e.content);
            RemoteChecksumResponse {
                checksum: format!("{digest:x}"),
                found: true,
                message: target.clone(),
            }
        }
        Some(_) => RemoteChecksumResponse {
            checksum: String::new(),
            found: false,
            message: "cannot checksum a directory".into(),
        },
        None => RemoteChecksumResponse {
            checksum: String::new(),
            found: false,
            message: format!("`{target}` not found"),
        },
    }
}

pub(crate) fn handle_upload(
    sender: String,
    req: protos::remote::fs::UploadRequest,
) -> protos::remote::fs::UploadResponse {
    use protos::remote::fs::UploadResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return UploadResponse {
                remote_path: String::new(),
                received_bytes: 0,
                success: false,
                message: "not in any freq; join or create one first".into(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let mut vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.remote_path);
    let now = now_ts();
    // 同名保护：目标已存在为目录时不能覆盖成文件。
    if let Some(e) = vfs.files.get(&target) {
        if e.is_dir {
            return UploadResponse {
                remote_path: target.clone(),
                received_bytes: 0,
                success: false,
                message: format!("`{target}` is a directory"),
            };
        }
    }
    // 父目录只按最后一个 '/' 切分（trim_end_matches 在“文件名与上级目录同名”时
    // 会把多级后缀反复剥掉，导致父目录缺失、文件无法正常处理）。
    let dir = match target.rfind('/') {
        Some(i) if i > 0 => target[..i].to_string(),
        _ => "/".to_string(),
    };
    vfs_ensure_dirs(&mut vfs, &dir, now);
    vfs.files.insert(
        target.clone(),
        VfsEntry {
            is_dir: false,
            size: req.content.len() as u64,
            content: req.content.clone(),
            modified_at: now,
        },
    );
    UploadResponse {
        remote_path: target.clone(),
        received_bytes: req.content.len() as i64,
        success: true,
        message: format!("uploaded {} bytes to {target}", req.content.len()),
    }
}

pub(crate) fn handle_download(
    sender: String,
    req: protos::remote::fs::DownloadRequest,
) -> protos::remote::fs::DownloadResponse {
    use protos::remote::fs::DownloadResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => {
            return DownloadResponse {
                total_size: 0,
                found: false,
                content: Vec::new(),
                message: "not in any freq; join or create one first".into(),
                files: Vec::new(),
            };
        }
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.remote_path);
    match vfs.files.get(&target) {
        Some(e) if !e.is_dir => DownloadResponse {
            total_size: e.size as i64,
            found: true,
            content: e.content.clone(),
            message: format!("{} bytes at {target}", e.size),
            files: Vec::new(),
        },
        Some(_) if req.recursive => {
            // 目录递归下载：收集该目录下所有普通文件（相对路径 + 内容）。
            use protos::remote::fs::DownloadEntry;
            let prefix = format!("{}/", target.trim_end_matches('/'));
            let mut entries = Vec::new();
            let mut total: i64 = 0;
            for (path, ent) in vfs.files.iter() {
                if ent.is_dir || !path.starts_with(&prefix) {
                    continue;
                }
                let rel = path[prefix.len()..].to_string();
                total += ent.size as i64;
                entries.push(DownloadEntry {
                    path: rel,
                    size: ent.size as i64,
                    content: ent.content.clone(),
                });
            }
            entries.sort_by(|a, b| a.path.cmp(&b.path));
            DownloadResponse {
                total_size: total,
                found: true,
                content: Vec::new(),
                message: format!("{} files ({} bytes) in directory {target}", entries.len(), total),
                files: entries,
            }
        }
        Some(_) => DownloadResponse {
            total_size: 0,
            found: false,
            content: Vec::new(),
            message: "cannot download a directory (use `-r` to download recursively)".into(),
            files: Vec::new(),
        },
        None => DownloadResponse {
            total_size: 0,
            found: false,
            content: Vec::new(),
            message: format!("`{target}` not found"),
            files: Vec::new(),
        },
    }
}
