use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    GameStart(GameStart),
    OpponentLeft,
    OpponentClick { pos: (usize, usize) },
    GameEnd { is_win: bool },
}

#[derive(Serialize, Deserialize)]
pub struct GameStart {
    pub target: [[Color; 3]; 3],
    pub board: [[Option<Color>; 5]; 5],
    pub opponent_board: [[Option<Color>; 5]; 5],
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Click { pos: (usize, usize) },
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    White,
    Yellow,
    Orange,
    Red,
    Green,
    Blue,
}
