#![cfg(not(feature = "ssr"))]

use leptos::*;

use crate::types::{BoardInner, BoardTiles, ClientMessage, Color, ServerMessage, Target};
use futures::{SinkExt, StreamExt};
use gloo_net::websocket::{futures::WebSocket, Message};
use tokio::{
    select,
    sync::{broadcast, mpsc},
};
use wasm_bindgen::{closure::Closure, JsCast};

type Void = std::convert::Infallible;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    WaitingForOpponent,
    Playing,
    WaitGameEnd, // target is matched, but server hasn't sent game end yet
    GameEnd { is_win: bool },
    OpponentLeft,
    ConnectionError,
}

impl State {
    fn is_end(&self) -> bool {
        matches!(
            self,
            State::GameEnd { .. } | State::OpponentLeft | State::ConnectionError
        )
    }
}

fn window_dimensions() -> (i32, i32) {
    let window = web_sys::window().expect("should have a window");
    let document = window.document().expect("no document");
    let root = document.document_element().expect("no root");

    (root.client_width(), root.client_height())
}

#[component]
pub(super) fn Game() -> impl IntoView {
    let (shutdown_tx, shutdown_rx) = broadcast::channel::<Void>(1);
    let mut send_shutdown = shutdown_rx;
    let mut recv_shutdown = shutdown_tx.subscribe();

    let shutdown_tx = store_value(Some(shutdown_tx));
    let do_shutdown = move || shutdown_tx.set_value(None);

    let window = web_sys::window().expect("should have a window");

    // set a callback for do_shutdown when the page is unloaded
    let shutdown_cb = Closure::<dyn Fn()>::new(do_shutdown);
    window.set_onbeforeunload(Some(shutdown_cb.as_ref().unchecked_ref()));
    // forgetting the callback leaks memory
    // store in the reactive system instead
    let _shutdown_cb = store_value(shutdown_cb);

    let host = window.location().host().expect("failed to get location");

    let (state, set_state) = create_signal(State::WaitingForOpponent);
    let (target, set_target) = create_signal(None::<[[Color; 3]; 3]>);
    let (board, set_board) = create_signal(None::<Board>);
    let (opponent_board, set_opponent_board) = create_signal(None::<Board>);
    let (dimensions, set_dimensions) = create_signal(window_dimensions());

    let resize_cb = Closure::<dyn Fn()>::new(move || {
        set_dimensions(window_dimensions());
    });
    window.set_onresize(Some(resize_cb.as_ref().unchecked_ref()));
    let _resize_cb = store_value(resize_cb);

    // we need to store the window for the reload callback
    let window = store_value(window);
    let reload = move |_| {
        _ = window().location().reload();
    };

    let ws = WebSocket::open(&format!("wss://{host}/connect")).expect("could not connect");
    let (mut tx, mut rx) = futures::StreamExt::split(ws);
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<ClientMessage>();

    // this wrapping is needed since msg_tx is not Copy
    let msg_tx = store_value(msg_tx);

    // websocket send loop
    spawn_local(async move {
        use futures::stream::SplitSink;

        log!("Entering send loop");

        let mut ping_interval = wasmtimer::tokio::interval(core::time::Duration::from_secs(50));

        async fn send_msg(
            msg: ClientMessage,
            tx: &mut SplitSink<WebSocket, Message>,
            set_state: WriteSignal<State>,
            do_shutdown: impl Fn(),
        ) {
            let msg = Message::Bytes(bincode::serialize(&msg).expect("failed to serialize"));
            if let Err(e) = tx.send(msg).await {
                log!("Failed to send message: {e}");
                set_state(State::ConnectionError);
                do_shutdown();
            }
        }

        loop {
            select! {
                msg = msg_rx.recv() => {
                    let Some(msg) = msg else { break; };
                    send_msg(msg, &mut tx, set_state, do_shutdown).await;
                }
                _ = ping_interval.tick() => {
                    send_msg(ClientMessage::Ping, &mut tx, set_state, do_shutdown).await;
                }
                _ = send_shutdown.recv() => {
                    break;
                }
            }
        }
        log!("Exiting send loop");
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

                // assumption: initial configuration will never contain the target
            }
            ServerMessage::OpponentLeft => {
                if !state.get_untracked().is_end() {
                    set_state(State::OpponentLeft);
                    do_shutdown();
                }
            }
            ServerMessage::OpponentClick { pos } => {
                if state.get_untracked() != State::Playing {
                    log!("Got opponent click but not playing");
                    return;
                }

                set_opponent_board.update(|board| {
                    board.as_mut().expect("playing but no board").click_pos(pos);
                });
            }
            ServerMessage::GameEnd { is_win } => {
                if matches!(state.get_untracked(), State::Playing | State::WaitGameEnd) {
                    set_state(State::GameEnd { is_win });
                    do_shutdown();
                } else {
                    log!("Got game end but not playing");
                }
            }
        }
    };

    // websocket receive loop
    spawn_local(async move {
        'outer: loop {
            select! {
                msg = rx.next() => {
                    let Some(msg) = msg else { break; };
                    let msg = 'msg: {
                        match msg {
                            Ok(Message::Bytes(msg)) => break 'msg msg,
                            Ok(msg) => log!("Unexpected message: {msg:?}"),
                            Err(e) => log!("Receive error: {e}"),
                        };
                        set_state(State::ConnectionError);
                        do_shutdown();
                        break 'outer;
                    };
                    let msg: ServerMessage = bincode::deserialize(&msg).expect("failed to deserialize");
                    handle_server_message(msg);
                }
                _ = recv_shutdown.recv() => {
                    break;
                }
            }
        }
    });

    let handle_click = move |idx: usize| {
        if state() != State::Playing {
            return;
        }
        set_board.update(|board| {
            let board = board.as_mut().expect("playing but no board");
            let pos = board.locations[idx];
            let updated = board.click_pos(pos);
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

    let target_view = move || {
        target.get()
            .map(|target| {
                target.into_iter().enumerate().flat_map(|(i, row)| {
                    row.into_iter()
                        .enumerate()
                        .map(move |(j, color)| view! {
                            <div class={format!("tile {color}", color = color_string(color))} style={format!("--row: {i}; --col: {j};")} />
                        })
                })
            })
            .into_iter()
            .flatten()
            .collect_view()
    };

    fn board_iter(
        board: ReadSignal<Option<Board>>,
    ) -> impl Iterator<Item = (usize, impl Fn() -> TileView + Copy)> {
        let range = if board.with(|board| board.is_some()) {
            0..24
        } else {
            0..0
        };

        range.into_iter().map(move |idx| {
            (idx, move || {
                board.with(move |board| {
                    let board = board.as_ref().unwrap();
                    let pos = board.locations[idx];
                    let tile = board.inner.tiles[pos.0][pos.1].unwrap();
                    TileView { pos, tile }
                })
            })
        })
    }

    fn make_board_view(
        board: ReadSignal<Option<Board>>,
        handle_click: impl Fn(usize) + 'static + Copy,
    ) -> impl IntoView {
        view! {
            <For
                each=move || board_iter(board)
                key=|&(idx, _)| idx
                view=move |(idx, data)| {
                    let pos = move || data().pos;
                    let color = move || data().tile.color;
                    let i = move || pos().0;
                    let j = move || pos().1;

                    view! {
                        <div class={move || format!("tile {color}", color = color_string(color()))} style={move || format!("--row: {i}; --col: {j};", i = i(), j = j())} on:click={move |_| handle_click(idx)} />
                    }
                }
            />
        }
    }

    let board_view = make_board_view(board, handle_click);
    let opponent_board_view = make_board_view(opponent_board, |_| {});

    let state_view = move || {
        let message = match state.get() {
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
        let button = matches!(state.get(), State::GameEnd { .. } | State::OpponentLeft)
            .then(|| view! { <button class="button" on:click=reload>"Play again"</button> });
        Some(view! {
            <div class="state">
                <span>{message}</span>
                {button}
            </div>
        })
    };

    game_view(
        dimensions,
        target_view,
        board_view,
        opponent_board_view,
        state_view,
    )
}

fn game_view(
    dimensions: ReadSignal<(i32, i32)>,
    target_view: impl IntoView,
    board_view: impl IntoView,
    opponent_board_view: impl IntoView,
    state_view: impl IntoView,
) -> impl IntoView {
    view! {
        <div class="background" style={move || format!("--screen-x: {x}; --screen-y: {y}", x = dimensions.get().0, y = dimensions.get().1)}>
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

    fn click_pos(&mut self, pos: (usize, usize)) -> bool {
        use crate::utils::slide;

        let Self {
            locations,
            inner: BoardInner { tiles, hole },
        } = self;
        let update = |old: (usize, usize), new: (usize, usize)| {
            locations[tiles[old.0][old.1].unwrap().idx] = new;
            tiles[new.0][new.1] = tiles[old.0][old.1];
        };

        if !slide(pos, *hole, update) {
            return false;
        }

        *hole = pos;
        true
    }
}

#[derive(Debug, Clone, Copy)]
struct TileView {
    pos: (usize, usize),
    tile: Tile,
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
