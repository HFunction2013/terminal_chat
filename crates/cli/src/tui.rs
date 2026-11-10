//! `--tui` 模式：基于 ratatui 的全屏界面。
//!
//! 布局：
//! ```text
//! ┌─ tc-cli ──────────────────────────────┐
//! │ 日志区（服务端推送 + 命令执行记录）      │
//! ├─ input ───────────────────────────────┤
//! │ tc> 正在输入...                        │
//! ├─ status ──────────────────────────────┤
//! │ Enter 执行 / PgUp PgDn 滚动 / Esc 退出 │
//! └───────────────────────────────────────┘
//! ```
use anyhow::Result;
use cli_core_types::RunCommandFn;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use std::io::stdout;
use std::time::Duration;

/// 拉取积压推送的 FFI 符号类型。
pub type PushDrainFn = unsafe extern "C" fn() -> safer_ffi::vec::Vec<safer_ffi::String>;

struct App {
    log: Vec<String>,
    input: String,
    cursor: usize,
    /// 日志区向上滚动偏移（0 = 贴底）。
    scroll: usize,
    status: String,
}

impl App {
    fn new() -> Self {
        let mut log = Vec::new();
        log.push("[tui] tc-cli TUI mode. Type a command and press Enter.".to_string());
        Self {
            log,
            input: String::new(),
            cursor: 0,
            scroll: 0,
            status: now_status(),
        }
    }

    fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }

    fn push_log(&mut self, text: String) {
        for line in text.lines() {
            self.log.push(line.to_string());
        }
        if self.scroll > 0 {
            // 保持用户正在看的相对位置。
            self.scroll += text.lines().count().max(1);
        }
    }
}

fn now_status() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("tc-cli | --tui | {}", d.as_secs()),
        Err(_) => "tc-cli | --tui".to_string(),
    }
}

/// 运行 TUI 主循环。在 alternate screen 中渲染；执行命令时临时退出全屏，
/// 让命令输出保留在终端历史里（与 vim `:!cmd` 的行为一致）。
pub fn run(run_command: &RunCommandFn, take_pushes: &PushDrainFn) -> Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("--tui requires an interactive terminal (stdout is not a tty)");
    }
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = event_loop(&mut terminal, &mut app, run_command, take_pushes);

    // 恢复终端
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    res
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    run_command: &RunCommandFn,
    take_pushes: &PushDrainFn,
) -> Result<()> {
    loop {
        // 取积压推送（srv 主动推送由 cli-core 队列中转）
        let pushes: Vec<safer_ffi::String> = unsafe { take_pushes() }.into();
        for p in pushes {
            app.push_log(format!("!! {}", p));
        }
        app.status = now_status();

        terminal.draw(|f| draw(f, app))?;

        // 约 100ms 刷新一次，兼顾推送及时性与 CPU
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? else {
            continue;
        };

        // Ctrl-C / Esc 退出
        if (modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c'))
            || code == KeyCode::Esc
        {
            return Ok(());
        }
        match code {
            KeyCode::Enter => {
                let cmd = app.input.trim().to_string();
                app.input.clear();
                app.cursor = 0;
                app.scroll = 0;
                if cmd.is_empty() {
                    continue;
                }
                app.push_log(format!("tc> {cmd}"));
                if cmd == "exit" || cmd == "quit" || cmd == "q" {
                    return Ok(());
                }
                // 退出全屏 -> 执行命令（输出留在终端历史）-> 回到全屏
                disable_raw_mode().ok();
                execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
                println!("\n[tui] running: {cmd}\n");
                match shell_words::split(&cmd) {
                    Ok(args) => {
                        let args_ffi: safer_ffi::vec::Vec<safer_ffi::String> =
                            args.into_iter().map(|s| s.into()).collect::<Vec<_>>().into();
                        let result = run_command(&args_ffi);
                        if result.code != 0 {
                            eprintln!("{}", result.message);
                        }
                        println!("[tui] exit code: {}", result.code);
                    }
                    Err(e) => eprintln!("[tui] parse error: {e}"),
                }
                print!("\n[tui] press Enter to return...");
                // 等待用户确认，避免输出一闪而过
                let _ = crossterm::event::read();
                enable_raw_mode()?;
                execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                terminal.clear()?;
                app.push_log(format!("[tui] done: {cmd}"));
            }
            KeyCode::Backspace => app.backspace(),
            KeyCode::Delete => app.delete(),
            KeyCode::Left => app.move_left(),
            KeyCode::Right => app.move_right(),
            KeyCode::Home => app.cursor = 0,
            KeyCode::End => app.cursor = app.input.len(),
            KeyCode::Up => app.scroll = app.scroll.saturating_add(1),
            KeyCode::Down => app.scroll = app.scroll.saturating_sub(1),
            KeyCode::PageUp => app.scroll = app.scroll.saturating_add(10),
            KeyCode::PageDown => app.scroll = app.scroll.saturating_sub(10),
            KeyCode::Char(c) => app.insert_char(c),
            _ => {}
        }
    }
}

fn draw(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    // 日志区
    let visible = chunks[0].height.saturating_sub(2) as usize;
    let total = app.log.len();
    let start = total.saturating_sub(visible).saturating_sub(app.scroll);
    let end = (start + visible).min(total);
    let lines: Vec<Line> = app.log[start..end]
        .iter()
        .map(|s| Line::from(Span::raw(s.clone())))
        .collect();
    let log = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" log "))
        .wrap(Wrap { trim: false });
    f.render_widget(log, chunks[0]);

    // 输入区（自绘光标）
    let mut spans: Vec<Span> = vec![Span::styled("tc> ", Style::default().fg(Color::Cyan))];
    let (before, rest) = app.input.split_at(app.cursor);
    spans.push(Span::raw(before.to_string()));
    let cursor_char = rest.chars().next().unwrap_or(' ');
    spans.push(Span::styled(
        cursor_char.to_string(),
        Style::default().add_modifier(Modifier::REVERSED),
    ));
    if rest.chars().count() > 1 {
        spans.push(Span::raw(rest.chars().skip(1).collect::<String>()));
    }
    let input = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(" input "));
    f.render_widget(input, chunks[1]);

    // 状态栏
    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} ", app.status),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw("  Enter run | PgUp/PgDn scroll | Esc/Ctrl-C quit | exit"),
    ]));
    f.render_widget(status, chunks[2]);
}
