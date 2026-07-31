use deeplx::{Config, DeepLX};
use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().expect("failed to create tokio runtime"))
}

pub fn translate(text: &str, from: &str, to: &str) -> Option<String> {
    rt().block_on(async {
        let translator = DeepLX::new(Config::default());
        translator.translate(from, to, text, None).await.ok().map(|r| r.data)
    })
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

        let translated = translator.translate("en", "zh", TC_CLI_HELP, None).await?.data;

        println!("===== 翻译结果 =====");
        println!("{}", translated);

        assert!(!translated.is_empty());
        assert!(translated.contains("用法") || translated.contains("命令"));

        Ok(())
    }
}
