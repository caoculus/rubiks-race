use cfg_if::cfg_if;

use leptos::*;
use leptos_meta::*;
use leptos_router::*;

// use crate::types::{Board, Color};
use crate::types::{BoardInner, BoardTiles, Color, Target};

#[component]
pub fn App(cx: Scope) -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context(cx);

    view! {
        cx,

        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/start-axum.css"/>

        // sets the document title
        <Title text="Rubik's Race"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes>
                    <Route path="" view=|cx| view! { cx, <HomePage/> }/>
                    <Route path="/game" view=|cx| view! { cx, <Game/> }/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Game(cx: Scope) -> impl IntoView {
    #[allow(unused)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum State {
        WaitingForOpponent,
        Playing,
        WaitGameEnd, // target is matched, but server hasn't sent game end yet
        GameEnd { is_win: bool },
        OpponentLeft,
        ConnectionError,
    }

    // cfg_if!(
    //     if #[cfg(not(feature = "ssr"))] {
    use crate::types::{ClientMessage, ServerMessage};
    use futures::{SinkExt, StreamExt};
    use gloo_net::websocket::{futures::WebSocket, Message};
    use tokio::sync::mpsc;

    impl State {
        fn is_end(&self) -> bool {
            matches!(
                self,
                State::GameEnd { .. } | State::OpponentLeft | State::ConnectionError
            )
        }
    }

    let (state, set_state) = create_signal(cx, State::WaitingForOpponent);
    let (target, set_target) = create_signal(cx, None::<[[Color; 3]; 3]>);
    let (board, set_board) = create_signal(cx, None::<Board>);
    let (opponent_board, set_opponent_board) = create_signal(cx, None::<Board>);

    let ws = WebSocket::open("ws://localhost:3000/connect").expect("could not connect");
    let (mut tx, mut rx) = futures::StreamExt::split(ws);
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<ClientMessage>();

    // this wrapping is needed since msg_tx is not Copy
    let msg_tx = store_value(cx, msg_tx);

    // websocket send loop
    // NOTE: this loop basically never ends... is this a problem?
    spawn_local(async move {
        while let Some(msg) = msg_rx.recv().await {
            let msg = Message::Bytes(bincode::serialize(&msg).expect("failed to serialize"));
            if let Err(e) = tx.send(msg).await {
                log!("Failed to send message: {e}");
                set_state(State::ConnectionError);
            }
        }
    });

    let handle_server_message = move |msg: ServerMessage| {
        match msg {
            ServerMessage::GameStart(start) => {
                if state.get_untracked() != State::WaitingForOpponent {
                    log!("Got game start but not waiting for opponent");
                    return;
                }

                set_target(Some(start.target));
                set_board(Some(Board::new(start.board)));
                set_opponent_board(Some(Board::new(start.opponent_board)));
                set_state(State::Playing);

                // i guess the assumption is that the initial configuration will never be a winning one?
            }
            ServerMessage::OpponentLeft => {
                if !state.get_untracked().is_end() {
                    set_state(State::OpponentLeft);
                }
            }
            ServerMessage::OpponentClick { pos } => {
                if state.get_untracked() != State::Playing {
                    log!("Got opponent click but not playing");
                    return;
                }

                set_opponent_board.update(|board| {
                    board
                        .as_mut()
                        .expect("playing but no board")
                        .click_tile(pos);
                });
            }
            ServerMessage::GameEnd { is_win } => {
                if matches!(state.get_untracked(), State::Playing | State::WaitGameEnd) {
                    set_state(State::GameEnd { is_win });
                } else {
                    log!("Got game end but not playing");
                }
            }
        }
    };

    // websocket receive loop
    spawn_local(async move {
        // weird false positive
        #[allow(clippy::never_loop)]
        while let Some(msg) = rx.next().await {
            if state.get_untracked().is_end() {
                break;
            }

            let msg = 'msg: {
                match msg {
                    Ok(Message::Bytes(msg)) => break 'msg msg,
                    Ok(msg) => log!("Unexpected message: {msg:?}"),
                    Err(e) => log!("Receive error: {e}"),
                };
                set_state(State::ConnectionError);
                return;
            };
            let msg: ServerMessage = bincode::deserialize(&msg).expect("failed to deserialize");
            handle_server_message(msg);
        }
    });

    let handle_click = move |pos: (usize, usize)| {
        if state() != State::Playing {
            return;
        }
        set_board.update(|board| {
            let board = board.as_mut().expect("playing but no board");
            let updated = board.click_tile(pos);
            if !updated {
                return;
            }
            _ = msg_tx.with_value(|msg_tx| msg_tx.send(ClientMessage::Click { pos }));
            let is_game_over = target.with(|target| {
                board.matches_target(target.as_ref().expect("playing but no target"))
            });

            if is_game_over {
                set_state(State::WaitGameEnd);
            }
        })
    };
    // } else {
    //     let (state, _) = create_signal(cx, State::WaitingForOpponent);
    //     let (target, _) = create_signal(cx, None::<[[Color; 3]; 3]>);
    //     let (board, _) = create_signal(cx, None::<Board<Tile>>);
    //     let (opponent_board, _) = create_signal(cx, None::<Board<Tile>>);

    //     let handle_click = |_| {};
    // }
    // );

    fn board_iter(tiles: BoardTiles<Tile>) -> impl Iterator<Item = (usize, usize, Tile)> + Clone {
        tiles.into_iter().enumerate().flat_map(|(i, row)| {
            row.into_iter()
                .enumerate()
                .filter_map(move |(j, tile)| tile.map(|tile| (i, j, tile)))
        })
    }

    let target_view = move || {
        target()
            .map(|target| {
                target.into_iter().enumerate().flat_map(|(i, row)| {
                    row.into_iter()
                        .enumerate()
                        .map(move |(j, color)| view! { cx,
                            <div class={format!("tile {color}", color = color_string(color))} style={format!("--row: {i}; --col: {j};")} />
                        })
                })
            })
            .into_iter()
            .flatten()
            .collect_view(cx)
    };

    let board_tiles = Signal::derive(cx, move || {
        board
            .with(|board| board.as_ref().map(|board| board_iter(board.inner.tiles)))
            .into_iter()
            .flatten()
    });

    let board_view = move || {
        log!("board_view called");
        view! { cx,
            <For
                each=board_tiles
                key=|(_, _, tile)| tile.idx
                view=move |cx, (i, j, tile)| {
                    log!("board_view called for ({i}, {j}, {tile:?})");
                    view! { cx,
                        <div class={format!("tile {color}", color = color_string(tile.color))} style={format!("--row: {i}; --col: {j};")} on:click={move |_| handle_click((i, j))} />
                    }
                }
            />
        }
    };

    let opponent_board_tiles = move || {
        opponent_board
            .with(|board| board.as_ref().map(|board| board_iter(board.inner.tiles)))
            .into_iter()
            .flatten()
    };

    let opponent_board_view = move || {
        view! { cx,
            <For
                each=opponent_board_tiles
                key=|(_, _, tile)| tile.idx
                view=move |cx, (i, j, tile)| {
                    view! { cx,
                        <div class={format!("tile {color}", color = color_string(tile.color))} style={format!("--row: {i}; --col: {j};")} />
                    }
                }
            />
        }
    };

    let state_view = move || {
        let message = match state() {
            State::WaitingForOpponent => "Waiting for opponent",
            State::GameEnd { is_win } => {
                if is_win {
                    "You win!"
                } else {
                    "You lose!"
                }
            }
            State::OpponentLeft => "Opponent left the game",
            State::ConnectionError => "Server connection error",
            _ => return None,
        };
        Some(view! { cx,
            <div class="state">{message}</div>
        })
    };

    view! { cx,
        <div class="background">
            <p class="target-label">"Target"</p>
            <div class="target">
                {target_view}
            </div>
            <div class="board">
                {board_view}
            </div>
            <p class="opponent-label">"Opponent"</p>
            <div class="opponent-board">
                {opponent_board_view}
            </div>
            {state_view}
        </div>
    }
}

fn color_string(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Yellow => "yellow",
        Color::Orange => "orange",
        Color::Red => "red",
        Color::Green => "green",
        Color::Blue => "blue",
    }
}

struct Board {
    locations: [(usize, usize); 24],
    inner: BoardInner<Tile>,
}

impl Board {
    fn new(inner: BoardInner) -> Self {
        let colors = inner.tiles.into_iter().enumerate().flat_map(|(i, row)| {
            row.into_iter()
                .enumerate()
                .filter_map(move |(j, tile)| tile.map(|tile| (i, j, tile)))
        });
        let mut locations: [(usize, usize); 24] = Default::default();
        let mut tiles: BoardTiles<Tile> = Default::default();

        for (idx, (loc, (i, j, color))) in (locations.iter_mut().zip(colors)).enumerate() {
            tiles[i][j] = Some(Tile { idx, color });
            *loc = (i, j);
        }

        Board {
            locations,
            inner: BoardInner {
                tiles,
                hole: inner.hole,
            },
        }
    }

    fn matches_target(&self, target: &Target) -> bool {
        self.inner.matches_target(target)
    }

    fn click_tile(&mut self, pos: (usize, usize)) -> bool {
        // TODO: better out of bounds handling?
        use std::cmp::Ordering;

        let Self {
            locations,
            inner: BoardInner { tiles, hole },
        } = self;
        let (row_cmp, col_cmp) = (pos.0.cmp(&hole.0), pos.1.cmp(&hole.1));

        match (row_cmp, col_cmp) {
            // same row
            (Ordering::Equal, _) => {
                let row = &mut tiles[pos.0];
                match col_cmp {
                    Ordering::Less => {
                        for i in (pos.1..hole.1).rev() {
                            locations[row[i].unwrap().idx].1 = i + 1;
                            row[i + 1] = row[i];
                        }
                    }
                    Ordering::Greater => {
                        for i in hole.1..pos.1 {
                            locations[row[i + 1].unwrap().idx].1 = i;
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
                            locations[tiles[i][pos.1].unwrap().idx].0 = i + 1;
                            tiles[i + 1][pos.1] = tiles[i][pos.1];
                        }
                    }
                    Ordering::Greater => {
                        for i in hole.0..pos.0 {
                            locations[tiles[i + 1][pos.1].unwrap().idx].0 = i;
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

#[derive(Debug, Clone, Copy)]
struct Tile {
    idx: usize,
    color: Color,
}

impl From<Tile> for Color {
    fn from(value: Tile) -> Self {
        value.color
    }
}

// #[cfg(not(feature = "ssr"))]
// impl Board<Tile> {
//     fn new(colors: [[Option<Color>; 5]; 5]) -> Self {
//         let mut hole = None;
//         let mut next_idx = 0;
//         let mut tiles = [[None; 5]; 5];

//         for (i, row) in colors.into_iter().enumerate() {
//             for (j, color) in row.into_iter().enumerate() {
//                 match color {
//                     Some(color) => {
//                         tiles[i][j] = Some(Tile {
//                             idx: next_idx,
//                             color,
//                         });
//                         next_idx += 1;
//                     }
//                     None => {
//                         assert!(
//                             hole.replace((i, j)).is_none(),
//                             "board should only have one hole"
//                         );
//                     }
//                 }
//             }
//         }

//         Self {
//             tiles,
//             hole: hole.expect("no hole"),
//         }
//     }
// }

/// Renders the home page of your application.
#[component]
fn HomePage(cx: Scope) -> impl IntoView {
    // using Form is a workaround for a redirecting button
    view! { cx,
        <h1>"Rubik's Race"</h1>
        <Form method="GET" action="/game">
            <button class="button">"Play"</button>
        </Form>
    }
}
