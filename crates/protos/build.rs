use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    let protos: Vec<_> = glob::glob("./proto/**/*.proto")?
        .filter_map(|e| e.ok())
        .collect();

    let out_dir = std::env::var("OUT_DIR")?;

    if !protos.is_empty() {
        for proto in &protos {
            println!("cargo:rerun-if-changed={}", proto.display());
        }

        config.out_dir(&out_dir);
        config.compile_protos(&protos, &["./proto/"])?;
    }

    // 生成 OUT_DIR/lib.rs
    let mut lib_content = String::new();
    let out_path = Path::new(&out_dir);

    if out_path.exists() {
        let mut entries: Vec<_> = fs::read_dir(out_path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("rs")
                    && e.path().file_stem().and_then(|s| s.to_str()) != Some("lib")
            })
            .collect();
        
        entries.sort_by_key(|e| e.file_name());

        // 生成模块声明
        for entry in &entries {
            let path = entry.path();
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            let module_name = file_name.replace('.', "_");
            
            lib_content.push_str(&format!(
                "pub mod {0} {{
    include!(concat!(env!(\"OUT_DIR\"), \"/{1}.rs\"));
}}\n\n",
                module_name, file_name
            ));
        }

        // 为带有 package 前缀的文件名生成嵌套模块别名
        // 例如：models.system -> pub mod models { pub mod system { pub use super::models_system::*; } }
        for entry in &entries {
            let path = entry.path();
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            
            if file_name.contains('.') {
                let parts: Vec<&str> = file_name.split('.').collect();
                if parts.len() == 2 {
                    let parent = parts[0];
                    let child = parts[1];
                    let flat_module = file_name.replace('.', "_");
                    
                    // 检查父模块是否已经被声明（作为扁平模块）
                    let parent_as_flat = entries.iter().any(|e| {
                        e.path().file_stem().and_then(|s| s.to_str()) == Some(parent)
                    });
                    
                    if !parent_as_flat {
                        // 生成嵌套模块：pub mod parent { pub mod child { pub use super::parent_child::*; } }
                        lib_content.push_str(&format!(
                            "pub mod {0} {{
    pub mod {1} {{
        pub use super::super::{2}::*;
    }}
}}\n\n",
                            parent, child, flat_module
                        ));
                    }
                }
            }
        }
    }

    let lib_out_path = out_path.join("lib.rs");
    fs::write(&lib_out_path, lib_content)?;

    println!("cargo:rerun-if-changed={}", lib_out_path.display());

    Ok(())
}