use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter};

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    GameStart(GameStart),
    OpponentLeft,
    OpponentClick { pos: (usize, usize) },
    GameEnd { is_win: bool },
}

pub type Target = [[Color; 3]; 3];

#[derive(Serialize, Deserialize)]
pub struct GameStart {
    pub target: Target,
    pub board: BoardInner,
    pub opponent_board: BoardInner,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Click { pos: (usize, usize) },
    Ping,
}

#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumCount,
)]
pub enum Color {
    #[default]
    White,
    Yellow,
    Orange,
    Red,
    Green,
    Blue,
}

impl From<usize> for Color {
    fn from(value: usize) -> Self {
        match value {
            0 => Color::White,
            1 => Color::Yellow,
            2 => Color::Orange,
            3 => Color::Red,
            4 => Color::Green,
            5 => Color::Blue,
            _ => panic!("out of bounds"),
        }
    }
}

pub type BoardTiles<T = Color> = [[Option<T>; 5]; 5];

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct BoardInner<T = Color> {
    pub tiles: BoardTiles<T>,
    pub hole: (usize, usize),
}

impl<T> BoardInner<T>
where
    T: Into<Color> + Copy,
{
    pub fn matches_target(&self, target: &Target) -> bool {
        for (board_row, target_row) in self.tiles[1..=3].iter().zip(target) {
            for (tile, target_color) in board_row[1..=3].iter().zip(target_row) {
                if !tile
                    .map(|tile| tile.into() == *target_color)
                    .unwrap_or(false)
                {
                    return false;
                }
            }
        }
        true
    }
}
