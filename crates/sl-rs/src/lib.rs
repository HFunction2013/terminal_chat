use std::io::{stdout, StdoutLock, Write};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, io};

use crossterm::{
    cursor::{Hide, Show, MoveTo},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{self, Clear, ClearType, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use getopts::Options;

use crate::c51::C51;
use crate::common::*;
use crate::d51::D51;
use crate::logo::Logo;

mod c51;
mod common;
mod d51;
mod logo;

enum SLType {
    Logo,
    C51,
    D51,
}

pub struct Terminal<'a> {
    out: StdoutLock<'a>,
    pub cursor: (u16, u16),
    pub cols: i32,
    pub lines: i32,
}

impl<'a> Terminal<'a> {
    fn new(out: StdoutLock<'a>) -> Self {
        let (cols, lines) = terminal::size().unwrap_or((80, 24));
        Terminal {
            out,
            cursor: (0, 0),
            cols: cols as i32,
            lines: lines as i32,
        }
    }

    pub fn init(&mut self) -> io::Result<()> {
        execute!(self.out, Clear(ClearType::All), Hide)?;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        execute!(
            self.out,
            Clear(ClearType::All),
            MoveTo(0, self.lines as u16),
            Show
        )?;
        Ok(())
    }

    pub fn clear_all(&mut self) -> io::Result<()> {
        execute!(self.out, Clear(ClearType::All))?;
        Ok(())
    }

    pub fn mvaddstr(&mut self, y: i32, mut x: i32, str: &str) -> bool {
        let mut chars = str.chars();
        while x < 0 {
            chars.next();
            x += 1;
        }
        for c in chars {
            if x < 0 || x >= self.cols || y < 0 || y >= self.lines {
                return false;
            }
            
            if execute!(self.out, MoveTo(x as u16, y as u16)).is_err() {
                return false;
            }

            if write!(self.out, "{}", c).is_err() {
                return false;
            }

            x += 1;
        }

        true
    }
}

impl Drop for Terminal<'_> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

pub struct Config {
    pub accident: bool,
    pub fly: bool,
    pub smoke: bool,
    pub smoke_state: smoke::SmokeState,
    pub interruptable: bool,
}

pub trait Train {
    fn update(&mut self, terminal: &mut Terminal, x: i32) -> bool;
    fn get_smoke_state(&mut self) -> &mut smoke::SmokeState;
    fn config(&self) -> &Config;

    fn run(&mut self) -> io::Result<()> {
        let out = stdout();
        let mut terminal = Terminal::new(out.lock());
        terminal.init()?;
        
        let mut interrupted = false;
        let frame_duration = Duration::from_millis(40);
        let mut next_frame_time = Instant::now() + frame_duration;

        let mut x = terminal.cols;
        while !interrupted {
            if !self.update(&mut terminal, x) {
                break;
            }

            // 非阻塞检查按键
            while event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key_event) => {
                        if key_event.code == KeyCode::Char('c') 
                            && key_event.modifiers.contains(KeyModifiers::CONTROL) 
                        {
                            if self.config().interruptable {
                                interrupted = true;
                            }
                        }
                    }
                    _ => {}
                }
            }

            io::stdout().flush()?;
            
            if let Some(duration) = checked_duration_since(next_frame_time, Instant::now()) {
                thread::sleep(duration);
            }
            next_frame_time += frame_duration;
            x -= 1;
        }

        Ok(())
    }

    fn add_man(&self, terminal: &mut Terminal, y: i32, x: i32) {
        for i in 0..2 {
            let man_x = ((SL_LENGTH + x) / 12 % 2) as usize;
            terminal.mvaddstr(y + i, x, MAN[man_x][i as usize]);
        }
    }

    fn add_smoke(&mut self, terminal: &mut Terminal, y: i32, x: i32) {
        use crate::smoke::*;
        let state = self.get_smoke_state();
        let sum: usize = state.sum;
        let s = &mut state.s;

        if x % 4 == 0 {
            for i in 0..sum {
                let pattern = s[i].ptrn as usize;
                terminal.mvaddstr(s[i].y, s[i].x, ERASER[pattern]);
                s[i].y -= DY[pattern];
                s[i].x += DX[pattern];
                let pattern = if pattern < SMOKEPTNS - 1 {
                    s[i].ptrn += 1;
                    s[i].ptrn as usize
                } else {
                    pattern
                };

                terminal.mvaddstr(s[i].y, s[i].x, SMOKE[(s[i].kind) as usize][pattern]);
            }
            terminal.mvaddstr(y, x, SMOKE[sum % 2][0]);
            s[sum].y = y;
            s[sum].x = x;
            s[sum].ptrn = 0;
            s[sum].kind = (sum % 2) as i32;
            state.sum = sum + 1;
        }
    }
}

fn checked_duration_since(s: Instant, earlier: Instant) -> Option<Duration> {
    if s > earlier {
        Some(s - earlier)
    } else {
        None
    }
}

fn print_usage(program: &str, opts: &Options) {
    println!("{}", opts.usage(&format!("Usage:\n {} [options]", program)));
}

pub fn run_sl() -> io::Result<()> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("l", "logo", "select logo");
    opts.optflag("c", "c51", "select C51");
    opts.optflag("F", "fly", "enable fly mode");
    opts.optflag("a", "accident", "enable accident mode");
    opts.optflag("s", "no-smoke", "disable smoke mode");
    opts.optflag("i", "interrupt", "enable Ctrl-C interrupt");
    opts.optflag("", "help", "show this usage message.");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(_) => {
            print_usage(&program, &opts);
            disable_raw_mode()?;
            return Ok(());
        }
    };
    if matches.opt_present("help") {
        print_usage(&program, &opts);
        disable_raw_mode()?;
        return Ok(());
    }
    let sl_type = if matches.opt_present("logo") {
        SLType::Logo
    } else if matches.opt_present("c51") {
        SLType::C51
    } else {
        SLType::D51
    };

    let conf = Config {
        accident: matches.opt_present("accident"),
        fly: matches.opt_present("fly"),
        smoke: !matches.opt_present("no-smoke"),
        smoke_state: smoke::SmokeState::default(),
        interruptable: matches.opt_present("interrupt"),
    };
    
    let result = match sl_type {
        SLType::Logo => Logo::new(conf).run(),
        SLType::C51 => C51::new(conf).run(),
        SLType::D51 => D51::new(conf).run(),
    };

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    result
}