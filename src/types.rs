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
    pub board: [[Option<Color>; 5]; 5],
    pub opponent_board: [[Option<Color>; 5]; 5],
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Click { pos: (usize, usize) },
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

pub struct Board<T> {
    pub tiles: [[Option<T>; 5]; 5],
    pub hole: (usize, usize),
}

impl<T> std::fmt::Debug for Board<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Board")
            .field("tiles", &self.tiles)
            .field("hole", &self.hole)
            .finish()
    }
}

impl<T> Clone for Board<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            tiles: self.tiles.clone(),
            hole: self.hole,
        }
    }
}

impl<T> Board<T>
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

impl<T> Board<T>
where
    T: Copy,
{
    // returns whether an update happened
    pub fn click_tile(&mut self, pos: (usize, usize)) -> bool {
        // TODO: better out of bounds handling?
        use std::cmp::Ordering;

        let Self { tiles, hole } = self;
        let (row_cmp, col_cmp) = (pos.0.cmp(&hole.0), pos.1.cmp(&hole.1));

        match (row_cmp, col_cmp) {
            // same row
            (Ordering::Equal, _) => {
                let row = &mut tiles[pos.0];
                match col_cmp {
                    Ordering::Less => {
                        for i in (pos.1..hole.1).rev() {
                            row[i + 1] = row[i];
                        }
                    }
                    Ordering::Greater => {
                        for i in hole.1..pos.1 {
                            row[i] = row[i + 1];
                        }
                    }
                    _ => return false,
                }
                row[pos.1] = None;
            }
            (_, Ordering::Equal) => {
                match row_cmp {
                    Ordering::Less => {
                        for i in (pos.0..hole.0).rev() {
                            tiles[i + 1][pos.1] = tiles[i][pos.1]
                        }
                    }
                    Ordering::Greater => {
                        for i in hole.0..pos.0 {
                            tiles[i][pos.1] = tiles[i + 1][pos.1]
                        }
                    }
                    _ => return false,
                }
                tiles[pos.0][pos.1] = None;
            }
            _ => return false,
        }

        *hole = pos;
        true
    }
}
