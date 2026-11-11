// upload.rs
// Upload file or folder to server
use crate::commands::CommandExecutor;
#[allow(unused_imports)]
use anyhow::{Result, anyhow};
use clap::ArgMatches;

pub struct UploadCommand;

impl UploadCommand {
    fn upload_one(&self, local: &str, remote: String) -> Result<()> {
        let content = std::fs::read(local).map_err(|e| anyhow!("cannot read `{local}`: {e}"))?;
        let req = protos::remote::fs::UploadRequest {
            local_path: local.to_string(),
            remote_path: remote.clone(),
            total_size: content.len() as i64,
            content,
        };
        let _resp: protos::remote::fs::UploadResponse = crate::connection::rpc("upload", &req)?;
        Ok(())
    }

    /// 递归收集目录下所有普通文件：(相对路径, 绝对路径)。
    fn collect_files(
        dir: &std::path::Path,
        rel: &str,
        out: &mut Vec<(String, String)>,
    ) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let rel_child = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if path.is_dir() {
                Self::collect_files(&path, &rel_child, out)?;
            } else {
                out.push((rel_child, path.to_string_lossy().to_string()));
            }
        }
        Ok(())
    }

    #[allow(unused_variables)]
    pub fn execute(&self, local_path: String, remote_path: Option<String>) -> Result<()> {
        let lib = crate::LIB.get().expect("`cli-core` not initialized");
        cli_core_macros::load_core_symbols!(lib, print_content);
        let base_name = |p: &str| p.rsplit(['/', '\\']).next().unwrap_or("file").to_string();
        let meta = std::fs::metadata(&local_path)
            .map_err(|e| anyhow!("cannot stat `{local_path}`: {e}"))?;
        if meta.is_dir() {
            // 目录上传：递归逐个文件上传到 remote/<相对路径>，保留目录结构。
            let base = remote_path.unwrap_or_else(|| base_name(&local_path));
            let mut files = Vec::new();
            Self::collect_files(std::path::Path::new(&local_path), "", &mut files)?;
            let mut ok = 0usize;
            for (rel, abs) in &files {
                let remote = format!("{}/{}", base.trim_end_matches('/'), rel);
                self.upload_one(abs, remote)?;
                ok += 1;
            }
            print_content(
                &format!("[*] uploaded {ok} files from `{local_path}` to `{base}/`").into(),
            );
            Ok(())
        } else {
            let remote = remote_path.unwrap_or_else(|| base_name(&local_path));
            self.upload_one(&local_path, remote.clone())?;
            print_content(&format!("[*] uploaded `{local_path}` to `{remote}`").into());
            Ok(())
        }
    }
}

impl CommandExecutor for UploadCommand {
    fn name(&self) -> &'static str {
        "upload"
    }

    #[allow(unused_variables)]
    fn run(&self, matches: &ArgMatches) -> Result<()> {
        let local_path = matches
            .get_one::<String>("local_path")
            .ok_or_else(|| anyhow!("Missing required argument: local_path"))?
            .clone();
        let remote_path = matches.get_one::<String>("remote_path").cloned();
        self.execute(local_path, remote_path)
    }
}
