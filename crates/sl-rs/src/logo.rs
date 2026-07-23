use super::{ Config, Train };
use crate::common::*;
use crate::Terminal;

const LOGOHEIGHT: i32 =    	 6;
const LOGOFUNNEL: i32 =  	 4;
const LOGOLENGTH: i32 =      SL_LENGTH;
const LOGOPATTERNS: i32 =	 6;

const LOGO1: &str = "     ++      +------ ";
const LOGO2: &str = "     ||      |+-+ |  ";
const LOGO3: &str = "   /---------|| | |  ";
const LOGO4: &str = "  + ========  +-+ |  ";


const LWHL11: &str = " _|--O========O~\\-+  ";
const LWHL12: &str = "//// \\_/      \\_/    ";

const LWHL21: &str = " _|--/O========O\\-+  ";
const LWHL22: &str = "//// \\_/      \\_/    ";

const LWHL31: &str = " _|--/~O========O-+  ";
const LWHL32: &str = "//// \\_/      \\_/    ";

const LWHL41: &str = " _|--/~\\------/~\\-+  ";
const LWHL42: &str = "//// \\_O========O    ";

const LWHL51: &str = " _|--/~\\------/~\\-+  ";
const LWHL52: &str = "//// \\O========O/    ";

const LWHL61: &str = " _|--/~\\------/~\\-+  ";
const LWHL62: &str = "//// O========O_/    ";

const LCOAL1: &str = "____                 ";
const LCOAL2: &str = "|   \\@@@@@@@@@@@     ";
const LCOAL3: &str = "|    \\@@@@@@@@@@@@@_ ";
const LCOAL4: &str = "|                  | ";
const LCOAL5: &str = "|__________________| ";
const LCOAL6: &str = "   (O)       (O)     ";

const LCAR1: &str = "____________________ ";
const LCAR2: &str = "|  ___ ___ ___ ___ | ";
const LCAR3: &str = "|  |_| |_| |_| |_| | ";
const LCAR4: &str = "|__________________| ";
const LCAR5: &str = "|__________________| ";
const LCAR6: &str = "   (O)        (O)    ";

const DELLN: &str = "                     ";

const SL: [[&str; (LOGOHEIGHT + 1) as usize]; LOGOPATTERNS as usize] =
    [[LOGO1, LOGO2, LOGO3, LOGO4, LWHL11, LWHL12, DELLN],
    [LOGO1, LOGO2, LOGO3, LOGO4, LWHL21, LWHL22, DELLN],
    [LOGO1, LOGO2, LOGO3, LOGO4, LWHL31, LWHL32, DELLN],
    [LOGO1, LOGO2, LOGO3, LOGO4, LWHL41, LWHL42, DELLN],
    [LOGO1, LOGO2, LOGO3, LOGO4, LWHL51, LWHL52, DELLN],
    [LOGO1, LOGO2, LOGO3, LOGO4, LWHL61, LWHL62, DELLN]];


const COAL: [&str; (LOGOHEIGHT + 1) as usize] =
    [LCOAL1, LCOAL2, LCOAL3, LCOAL4, LCOAL5, LCOAL6, DELLN];

const CAR: [&str; (LOGOHEIGHT + 1) as usize] =
    [LCAR1, LCAR2, LCAR3, LCAR4, LCAR5, LCAR6, DELLN];

pub struct Logo {
    conf: Config
}

impl Logo {
    pub fn new(c: Config) -> Logo {
        Logo { conf: c }
    }
}

impl Train for Logo {
    fn update(&mut self, terminal: &mut Terminal, x: i32) -> bool {
        if x < -LOGOLENGTH { return false }
        let y;
        let py1;
        let py2;
        let py3;

        if self.conf.fly {
            y = (x / 6) + terminal.lines - (terminal.cols / 6) - LOGOHEIGHT;
            py1 = 2;  py2 = 4;  py3 = 6;
        } else {
            y = terminal.lines / 2 - 3;
            py1 = 0;  py2 = 0;  py3 = 0;
        }

        for i in 0..=LOGOHEIGHT {
            let idx = i as usize;
            let sl_y = ((LOGOLENGTH + x) / 3 % LOGOPATTERNS) as usize;
            terminal.mvaddstr(y + i, x, SL[sl_y][idx]);
            terminal.mvaddstr(y + i + py1, x + 21, COAL[idx]);
            terminal.mvaddstr(y + i + py2, x + 42, CAR[idx]);
            terminal.mvaddstr(y + i + py3, x + 63, CAR[idx]);
        }
        if self.conf.accident {
            self.add_man(terminal, y + 1, x + 14);
            self.add_man(terminal, y + 1 + py2, x + 45);
            self.add_man(terminal, y + 1 + py2, x + 53);
            self.add_man(terminal, y + 1 + py3, x + 66);
            self.add_man(terminal, y + 1 + py3, x + 74);
        }
        if self.conf.smoke {
            self.add_smoke(terminal, y - 1, x + LOGOFUNNEL);
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
