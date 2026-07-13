use deeplx::{Config, DeepLX};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use tokio::runtime::Runtime;
/// # Safety
/// 翻译函数
/// text: UTF-8 C string
/// from: 源语言（如 "en"）
/// to:   目标语言（如 "zh"）
/// 返回：UTF-8 C string（调用方负责 free）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn translate(
    text: *const c_char,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    if text.is_null() || from.is_null() || to.is_null() {
        return ptr::null_mut();
    }

    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned();
    let from = unsafe { CStr::from_ptr(from) }.to_string_lossy().into_owned();
    let to = unsafe { CStr::from_ptr(to) }.to_string_lossy().into_owned();

    // 创建 tokio runtime（FFI 不能用 async）
    let rt = Runtime::new().unwrap();

    let result = rt.block_on(async {
        let translator = DeepLX::new(Config::default());
        translator
            .translate(&from, &to, &text, None)
            .await
    });

    match result {
        Ok(res) => {
            let s = CString::new(res.data).unwrap();
            s.into_raw()
        }
        Err(_) => ptr::null_mut(),
    }
}
/// # Safety
/// 释放字符串内存
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}
#[cfg(test)]
mod tests {
    use deeplx::{Config, DeepLX};

    const TC_CLI_HELP: &str = r#"Usage: tc-cli [COMMAND]

    Commands:
      setg          Set global options
      set           Set session options
      route         Trace route to server
      proxy_off     Turn off system proxy
      show          Show runtime information
      join_freq     Join a target frequency channel
      create_freq   Create a new frequency channel
      background    Background current session and return to main console
      sessions      Multi-session manager
      load          Load Plugin
      unload        Unload Plugin
      clear         Clear the Screen
      tcpm          Terminal Chat Package Manager
      upload        Upload file or folder to server.
      download      Download file or folder to server.
      checksum      Get checksum of remote file.
      cd            Change the current remote directory to DIR.
      pwd           Get current remote directory.
      ls            List files in current directory.
      mkdir         Make directory in remote server.
      file_info     Get remote file infomation.
      leave_freq    Leave current frequency.
      call          Call an user
      msg           send message
      schedule_msg  Schedules message.
      cancel        cancel a task
      whoami        Get username.
      help          Print this message or the help of the given subcommand(s)
    
    Options:
      -h, --help     Print help
      -V, --version  Print version"#;

    #[tokio::test]
    async fn test_translate_cli_help() -> Result<(), Box<dyn std::error::Error>> {
        let translator = DeepLX::new(Config::default());

        let translated = translator
            .translate("en", "zh", TC_CLI_HELP, None)
            .await?
            .data;

        println!("===== 翻译结果 =====");
        println!("{}", translated);

        assert!(!translated.is_empty());
        assert!(translated.contains("用法") || translated.contains("命令"));

        Ok(())
    }
}