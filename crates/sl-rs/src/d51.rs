use super::{ Config, Train };
use crate::common::*;
use crate::Terminal;


const D51HEIGHT: i32 = 	10;
const D51FUNNEL: i32 = 	 7;
const D51LENGTH: i32 = 	83;
const D51PATTERNS: i32 = 6;


const D51STR1: &str = "      ====        ________                ___________ ";
const D51STR2: &str = "  _D _|  |_______/        \\__I_I_____===__|_________| ";
const D51STR3: &str = "   |(_)---  |   H\\________/ |   |        =|___ ___|   ";
const D51STR4: &str = "   /     |  |   H  |  |     |   |         ||_| |_||   ";
const D51STR5: &str = "  |      |  |   H  |__--------------------| [___] |   ";
const D51STR6: &str = "  | ________|___H__/__|_____/[][]~\\_______|       |   ";
const D51STR7: &str = "  |/ |   |-----------I_____I [][] []  D   |=======|__ ";

const D51WHL11: &str = "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ";
const D51WHL12: &str = " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ";
const D51WHL13: &str = "  \\_/      \\O=====O=====O=====O_/      \\_/            ";

const D51WHL21: &str = "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ";
const D51WHL22: &str = " |/-=|___|=O=====O=====O=====O   |_____/~\\___/        ";
const D51WHL23: &str = "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ";

const D51WHL31: &str = "__/ =| o |=-O=====O=====O=====O \\ ____Y___________|__ ";
const D51WHL32: &str = " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ";
const D51WHL33: &str = "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ";

const D51WHL41: &str = "__/ =| o |=-~O=====O=====O=====O\\ ____Y___________|__ ";
const D51WHL42: &str = " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ";
const D51WHL43: &str = "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ";

const D51WHL51: &str = "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ";
const D51WHL52: &str = " |/-=|___|=   O=====O=====O=====O|_____/~\\___/        ";
const D51WHL53: &str = "  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/            ";

const D51WHL61: &str = "__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|__ ";
const D51WHL62: &str = " |/-=|___|=    ||    ||    ||    |_____/~\\___/        ";
const D51WHL63: &str = "  \\_/      \\_O=====O=====O=====O/      \\_/            ";

const D51DEL: &str = "                                                      ";


const SL: [[&str; (D51HEIGHT + 1) as usize]; D51PATTERNS as usize] =
    [[D51STR1, D51STR2, D51STR3, D51STR4, D51STR5, D51STR6, D51STR7,
        D51WHL11, D51WHL12, D51WHL13, D51DEL],
        [D51STR1, D51STR2, D51STR3, D51STR4, D51STR5, D51STR6, D51STR7,
        D51WHL21, D51WHL22, D51WHL23, D51DEL],
        [D51STR1, D51STR2, D51STR3, D51STR4, D51STR5, D51STR6, D51STR7,
        D51WHL31, D51WHL32, D51WHL33, D51DEL],
        [D51STR1, D51STR2, D51STR3, D51STR4, D51STR5, D51STR6, D51STR7,
        D51WHL41, D51WHL42, D51WHL43, D51DEL],
        [D51STR1, D51STR2, D51STR3, D51STR4, D51STR5, D51STR6, D51STR7,
        D51WHL51, D51WHL52, D51WHL53, D51DEL],
        [D51STR1, D51STR2, D51STR3, D51STR4, D51STR5, D51STR6, D51STR7,
        D51WHL61, D51WHL62, D51WHL63, D51DEL]];


const COAL: [&str; (D51HEIGHT + 1) as usize] =
    [COAL01, COAL02, COAL03, COAL04, COAL05,
        COAL06, COAL07, COAL08, COAL09, COAL10, COALDEL];

pub struct D51 {
    conf: Config
}

impl D51 {
    pub fn new(c: Config) -> D51 {
        D51 { conf: c }
    }
}

impl Train for D51 {
    fn update(&mut self, terminal: &mut Terminal, x: i32) -> bool {
        if x < -D51LENGTH { return false }
        let y;
        let dy;

        if self.conf.fly {
            y = (x / 7) + terminal.lines - (terminal.cols / 7) - D51HEIGHT;
            dy = 1;
        } else {
            y = terminal.lines / 2 - 5;
            dy = 0;
        }

        for i in 0..=D51HEIGHT {
            let idx = i as usize;
            let sl_y = ((D51LENGTH + x) % D51PATTERNS) as usize;
            terminal.mvaddstr(y + i, x, SL[sl_y][idx]);
            terminal.mvaddstr(y + i + dy, x + 53, COAL[idx]);
        }
        if self.conf.accident {
            self.add_man(terminal, y + 2, x + 43);
            self.add_man(terminal, y + 2, x + 47);
        }
        if self.conf.smoke {
            self.add_smoke(terminal, y - 1, x + D51FUNNEL);
        }

        true
    }

    fn get_smoke_state(&mut self) -> &mut smoke::SmokeState {
        &mut self.conf.smoke_state
    }

    fn config(&self) -> &Config {
        &self.conf
    }
}
