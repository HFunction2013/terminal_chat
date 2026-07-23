pub const SL_LENGTH: i32 = 84;

pub const MAN: [[&str; 2]; 2] = [["", "(O)"], ["Help!", "\\O/"]];

pub const COAL01: &str = "                              ";
pub const COAL02: &str = "                              ";
pub const COAL03: &str = "    _________________         ";
pub const COAL04: &str = "   _|                \\_____A  ";
pub const COAL05: &str = " =|                        |  ";
pub const COAL06: &str = " -|                        |  ";
pub const COAL07: &str = "__|________________________|_ ";
pub const COAL08: &str = "|__________________________|_ ";
pub const COAL09: &str = "   |_D__D__D_|  |_D__D__D_|   ";
pub const COAL10: &str = "    \\_/   \\_/    \\_/   \\_/    ";

pub const COALDEL: &str = "                              ";


pub mod smoke {
    pub const SMOKEPTNS: usize = 16;

    #[derive(Copy, Clone, Debug)]
    pub struct Smokes {
        pub y: i32,
        pub x: i32,
        pub ptrn: i32,
        pub kind: i32
    }

    pub const SMOKE: [[&str; SMOKEPTNS]; 2] =
        [["(   )", "(    )", "(    )", "(   )", "(  )",
          "(  )" , "( )"   , "( )"   , "()"   , "()"  ,
          "O"    , "O"     , "O"     , "O"    , "O"   ,
          " "                                          ],
         ["(@@@)", "(@@@@)", "(@@@@)", "(@@@)", "(@@)",
          "(@@)" , "(@)"   , "(@)"   , "@@"   , "@@"  ,
          "@"    , "@"     , "@"     , "@"    , "@"   ,
          " "                                          ]];

    pub const ERASER: [&str; SMOKEPTNS] =
        ["     ", "      ", "      ", "     ", "    ",
         "    " , "   "   , "   "   , "  "   , "  "  ,
         " "    , " "     , " "     , " "    , " "   ,
         " "                                          ];

    pub const DY: [i32; SMOKEPTNS] =
        [2, 1, 1, 1, 0, 0, 0, 0, 0, 0,
         0, 0, 0, 0, 0, 0];
    pub const DX: [i32; SMOKEPTNS] =
        [-2, -1, 0, 1, 1, 1, 1, 1, 2, 2,
          2, 2, 2, 3, 3, 3];

    pub struct SmokeState {
        pub sum: usize,
        pub s: [Smokes; 1000]
    }

    impl Default for SmokeState {
        fn default() -> SmokeState {
            SmokeState {
                sum: 0,
                s: [Smokes { x: 0, y: 0, ptrn: 0, kind: 0 }; 1000]
            }
        }
    }
}
