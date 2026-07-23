use super::{ Config, Train };
use crate::common::*;
use crate::Terminal;

const C51HEIGHT: i32 = 11;
const C51FUNNEL: i32 = 7;
const C51LENGTH: i32 = 87;
const C51PATTERNS: i32 = 6;

const C51DEL: &str = "                                                       ";

const C51STR1: &str = "        ___                                            ";
const C51STR2: &str = "       _|_|_  _     __       __             ___________";
const C51STR3: &str = "    D__/   \\_(_)___|  |__H__|  |_____I_Ii_()|_________|";
const C51STR4: &str = "     | `---'   |:: `--'  H  `--'         |  |___ ___|  ";
const C51STR5: &str = "    +|~~~~~~~~++::~~~~~~~H~~+=====+~~~~~~|~~||_| |_||  ";
const C51STR6: &str = "    ||        | ::       H  +=====+      |  |::  ...|  ";
const C51STR7: &str = "|    | _______|_::-----------------[][]-----|       |  ";

const C51WH61: &str = "| /~~ ||   |-----/~~~~\\  /[I_____I][][] --|||_______|__";
const C51WH62: &str = "------'|oOo|==[]=-     ||      ||      |  ||=======_|__";
const C51WH63: &str = "/~\\____|___|/~\\_|   O=======O=======O  |__|+-/~\\_|     ";
const C51WH64: &str = "\\_/         \\_/  \\____/  \\____/  \\____/      \\_/       ";

const C51WH51: &str = "| /~~ ||   |-----/~~~~\\  /[I_____I][][] --|||_______|__";
const C51WH52: &str = "------'|oOo|===[]=-    ||      ||      |  ||=======_|__";
const C51WH53: &str = "/~\\____|___|/~\\_|    O=======O=======O |__|+-/~\\_|     ";
const C51WH54: &str = "\\_/         \\_/  \\____/  \\____/  \\____/      \\_/       ";

const C51WH41: &str = "| /~~ ||   |-----/~~~~\\  /[I_____I][][] --|||_______|__";
const C51WH42: &str = "------'|oOo|===[]=- O=======O=======O  |  ||=======_|__";
const C51WH43: &str = "/~\\____|___|/~\\_|      ||      ||      |__|+-/~\\_|     ";
const C51WH44: &str = "\\_/         \\_/  \\____/  \\____/  \\____/      \\_/       ";

const C51WH31: &str = "| /~~ ||   |-----/~~~~\\  /[I_____I][][] --|||_______|__";
const C51WH32: &str = "------'|oOo|==[]=- O=======O=======O   |  ||=======_|__";
const C51WH33: &str = "/~\\____|___|/~\\_|      ||      ||      |__|+-/~\\_|     ";
const C51WH34: &str = "\\_/         \\_/  \\____/  \\____/  \\____/      \\_/       ";

const C51WH21: &str = "| /~~ ||   |-----/~~~~\\  /[I_____I][][] --|||_______|__";
const C51WH22: &str = "------'|oOo|=[]=- O=======O=======O    |  ||=======_|__";
const C51WH23: &str = "/~\\____|___|/~\\_|      ||      ||      |__|+-/~\\_|     ";
const C51WH24: &str = "\\_/         \\_/  \\____/  \\____/  \\____/      \\_/       ";

const C51WH11: &str = "| /~~ ||   |-----/~~~~\\  /[I_____I][][] --|||_______|__";
const C51WH12: &str = "------'|oOo|=[]=-      ||      ||      |  ||=======_|__";
const C51WH13: &str = "/~\\____|___|/~\\_|  O=======O=======O   |__|+-/~\\_|     ";
const C51WH14: &str = "\\_/         \\_/  \\____/  \\____/  \\____/      \\_/       ";


const SL: [[&str; (C51HEIGHT + 1) as usize]; C51PATTERNS as usize] =
    [[C51STR1, C51STR2, C51STR3, C51STR4, C51STR5, C51STR6, C51STR7,
        C51WH11, C51WH12, C51WH13, C51WH14, C51DEL],
        [C51STR1, C51STR2, C51STR3, C51STR4, C51STR5, C51STR6, C51STR7,
        C51WH21, C51WH22, C51WH23, C51WH24, C51DEL],
        [C51STR1, C51STR2, C51STR3, C51STR4, C51STR5, C51STR6, C51STR7,
        C51WH31, C51WH32, C51WH33, C51WH34, C51DEL],
        [C51STR1, C51STR2, C51STR3, C51STR4, C51STR5, C51STR6, C51STR7,
        C51WH41, C51WH42, C51WH43, C51WH44, C51DEL],
        [C51STR1, C51STR2, C51STR3, C51STR4, C51STR5, C51STR6, C51STR7,
        C51WH51, C51WH52, C51WH53, C51WH54, C51DEL],
        [C51STR1, C51STR2, C51STR3, C51STR4, C51STR5, C51STR6, C51STR7,
        C51WH61, C51WH62, C51WH63, C51WH64, C51DEL]];


const COAL: [&str; (C51HEIGHT + 1) as usize] =
    [COALDEL, COAL01, COAL02, COAL03, COAL04, COAL05,
        COAL06, COAL07, COAL08, COAL09, COAL10, COALDEL];

pub struct C51 {
    conf: Config
}

impl C51 {
    pub fn new(c: Config) -> C51 {
        C51 { conf: c }
    }
}

impl Train for C51 {
    fn update(&mut self, terminal: &mut Terminal, x: i32) -> bool {
        if x < -C51LENGTH { return false }
        let y;
        let dy;

        if self.conf.fly {
            y = (x / 7) + terminal.lines - (terminal.cols / 7) - C51HEIGHT;
            dy = 1;
        } else {
            y = terminal.lines / 2 - 5;
            dy = 0;
        }

        for i in 0..=C51HEIGHT {
            let idx = i as usize;
            let sl_y = ((C51LENGTH + x) % C51PATTERNS) as usize;
            terminal.mvaddstr(y + i, x, SL[sl_y][idx]);
            terminal.mvaddstr(y + i + dy, x + 55, COAL[idx]);
        }
        if self.conf.accident {
            self.add_man(terminal, y + 3, x + 45);
            self.add_man(terminal, y + 3, x + 49);
        }
        if self.conf.smoke {
            self.add_smoke(terminal, y - 1, x + C51FUNNEL);
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
