//! //! 远程文件系统（VFS 按 freq 共享）：pwd / ls / cd / mkdir / info / checksum / upload / download

use sha2::{Digest, Sha256};
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[allow(unused_imports)]
use crate::{STATE, VFS, NOTIFICATIONS, FreqRoom, Member, UserProfile, ServerState, VfsEntry, VfsUser, ensure_user, user_freq, user_part, display_user, valid_freq, now_ts, notify, mod_auth, norm_path, vfs_ensure_dirs};

pub(crate) fn handle_remote_pwd(sender: String, _req: protos::remote::fs::PwdRequest) -> protos::remote::fs::PwdResponse {
    use protos::remote::fs::PwdResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return PwdResponse { directory: String::new(), success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    PwdResponse {
        directory: vfs.cwd.clone(),
        success: true,
        message: vfs.cwd.clone(),
    }
}

pub(crate) fn handle_remote_ls(sender: String, _req: protos::remote::fs::LsRequest) -> protos::remote::fs::LsResponse {
    use protos::remote::fs::{FileEntry, LsResponse};
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return LsResponse { directory: String::new(), entries: vec![], success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let mut entries = Vec::new();
    let prefix = if vfs.cwd == "/" { String::new() } else { vfs.cwd.clone() };
    for (path, entry) in &vfs.files {
        if !path.starts_with(&format!("{prefix}/")) && path != &vfs.cwd {
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
        directory: vfs.cwd.clone(),
        entries,
        success: true,
        message: format!("listing {}", vfs.cwd),
    }
}

pub(crate) fn handle_remote_cd(sender: String, req: protos::remote::fs::CdRequest) -> protos::remote::fs::CdResponse {
    use protos::remote::fs::CdResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return CdResponse { directory: String::new(), success: false, message: "not in any freq; join or create one first".into() },
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
    CdResponse {
        directory: target.clone(),
        success: true,
        message: format!("now at {target}"),
    }
}

pub(crate) fn handle_remote_mkdir(sender: String, req: protos::remote::fs::MkdirRequest) -> protos::remote::fs::MkdirResponse {
    use protos::remote::fs::MkdirResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return MkdirResponse { success: false, message: "not in any freq; join or create one first".into() },
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

pub(crate) fn handle_file_info(sender: String, req: protos::remote::fs::RemoteFileInfoRequest) -> protos::remote::fs::RemoteFileInfoResponse {
    use protos::remote::fs::{RemoteFileInfo, RemoteFileInfoResponse};
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteFileInfoResponse { found: false, info: None, message: "not in any freq; join or create one first".into() },
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

pub(crate) fn handle_remote_checksum(sender: String, req: protos::remote::fs::RemoteChecksumRequest) -> protos::remote::fs::RemoteChecksumResponse {
    use protos::remote::fs::RemoteChecksumResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return RemoteChecksumResponse { checksum: String::new(), found: false, message: "not in any freq; join or create one first".into() },
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
        Some(_) => RemoteChecksumResponse { checksum: String::new(), found: false, message: "cannot checksum a directory".into() },
        None => RemoteChecksumResponse { checksum: String::new(), found: false, message: format!("`{target}` not found") },
    }
}

pub(crate) fn handle_upload(sender: String, req: protos::remote::fs::UploadRequest) -> protos::remote::fs::UploadResponse {
    use protos::remote::fs::UploadResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return UploadResponse { remote_path: String::new(), received_bytes: 0, success: false, message: "not in any freq; join or create one first".into() },
    };
    let mut all_vfs = VFS.lock().unwrap();
    let mut vfs = all_vfs
        .entry(freq.clone())
        .or_insert_with(|| VfsUser { cwd: "/".to_string(), files: HashMap::new() });
    let target = norm_path(&vfs.cwd, &req.remote_path);
    let now = now_ts();
    vfs_ensure_dirs(&mut vfs, &target, now);
    let name = target.rsplit('/').next().unwrap_or("file").to_string();
    let dir = target.trim_end_matches(&format!("/{name}"));
    if !dir.is_empty() {
        vfs_ensure_dirs(&mut vfs, dir, now);
    }
    vfs.files.insert(target.clone(), VfsEntry {
        is_dir: false,
        size: req.content.len() as u64,
        content: req.content.clone(),
        modified_at: now,
    });
    UploadResponse {
        remote_path: target.clone(),
        received_bytes: req.content.len() as i64,
        success: true,
        message: format!("uploaded {} bytes to {target}", req.content.len()),
    }
}

pub(crate) fn handle_download(sender: String, req: protos::remote::fs::DownloadRequest) -> protos::remote::fs::DownloadResponse {
    use protos::remote::fs::DownloadResponse;
    // VFS 按 freq 共享（无私有文件系统；disband 时销毁）。
    let freq = match user_freq(&STATE.lock().unwrap(), &sender) {
        Some(f) => f,
        None => return DownloadResponse { total_size: 0, found: false, content: Vec::new(), message: "not in any freq; join or create one first".into() },
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
        },
        Some(_) => DownloadResponse { total_size: 0, found: false, content: Vec::new(), message: "cannot download a directory".into() },
        None => DownloadResponse { total_size: 0, found: false, content: Vec::new(), message: format!("`{target}` not found") },
    }
}
