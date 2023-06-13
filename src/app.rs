use futures::{stream::SplitSink, SinkExt, StreamExt};

use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::*;
use leptos_meta::*;
use leptos_router::*;
use tokio::sync::mpsc;

use crate::types::{ClientMessage, Color, ServerMessage};

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
        <Title text="Welcome to Leptos"/>

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

// let messages = create_signal_from_stream(
//     cx,
//     rx.map_while(|msg| {
//         let Ok(Message::Bytes(msg)) = msg else { return None; };
//         let msg: ServerMessage = bincode::deserialize(&msg).expect("failed to deserialize");
//         Some(msg)
//     }),
// );

#[component]
fn Game(cx: Scope) -> impl IntoView {
    // TODO: game logic needs to move to cfg not ssr later

    let ws = WebSocket::open("ws://localhost:3000/connect").expect("could not connect");
    let (mut tx, mut rx) = futures::StreamExt::split(ws);
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<ClientMessage>();

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum State {
        WaitingForOpponent,
        Playing,
        GameEnd { is_win: bool },
        OpponentLeft,
        ConnectionError,
    }

    let (state, set_state) = create_signal(cx, State::WaitingForOpponent);
    let (target, set_target) = create_signal(cx, None::<[[Color; 3]; 3]>);
    let (board, set_board) = create_signal(cx, None::<Board<Tile>>);
    let (opponent_board, set_opponent_board) = create_signal(cx, None::<Board<Tile>>);
    // TODO: what should the board look like?
    // if we want to use the For component, then we need to associate an index with each tile

    // websocket send loop
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
                if state() != State::WaitingForOpponent {
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
                if matches!(state(), State::WaitingForOpponent | State::Playing) {
                    set_state(State::OpponentLeft);
                }
            }
            ServerMessage::OpponentClick { pos } => {
                //
            }
            ServerMessage::GameEnd { is_win } => {
                if state() == State::Playing {
                    set_state(State::GameEnd { is_win });
                } else {
                    log!("Got game end but not playing");
                }
            }
        }
        todo!()
    };

    // websocket receive loop
    // question: should game logic be handled in here?
    spawn_local(async move {
        // weird false positive
        #[allow(clippy::never_loop)]
        while let Some(msg) = rx.next().await {
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

    // view! { cx,
    //     <p>"The game would go here"</p>
    // }
    todo!()
}

struct Board<T> {
    tiles: [[Option<T>; 5]; 5],
    hole: (usize, usize),
}

impl Board<Tile> {
    fn new(colors: [[Option<Color>; 5]; 5]) -> Self {
        let mut hole = None;
        let mut next_idx = 0;
        let mut tiles = [[None; 5]; 5];

        for (i, row) in colors.into_iter().enumerate() {
            for (j, color) in row.into_iter().enumerate() {
                match color {
                    Some(color) => {
                        tiles[i][j] = Some(Tile {
                            idx: next_idx,
                            color,
                        });
                        next_idx += 1;
                    }
                    None => {
                        assert!(
                            hole.replace((i, j)).is_none(),
                            "board should only have one hole"
                        );
                    }
                }
            }
        }

        Self {
            tiles,
            hole: hole.expect("no hole"),
        }
    }
}

type Target = [[Color; 3]; 3];

impl<T> Board<T>
where
    T: Into<Color> + Copy,
{
    fn matches_target(&self, target: &Target) -> bool {
        for (board_row, target_row) in self.tiles[1..=3].iter().zip(target) {
            for (tile, target_color) in board_row.iter().zip(target_row) {
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
    fn click_tile(&mut self, pos: (usize, usize)) -> bool {
        use std::cmp::Ordering;

        let Self { tiles, hole } = self;
        let (row_cmp, col_cmp) = (pos.0.cmp(&hole.0), pos.1.cmp(&hole.1));

        match (row_cmp, col_cmp) {
            (Ordering::Equal, _) => {
                let row = &mut tiles[pos.0];
                match col_cmp {
                    Ordering::Less => {
                        for i in (pos.1 + 1..hole.1 + 1).rev() {
                            row[i + 1] = row[i];
                        }
                    }
                    Ordering::Greater => {
                        for i in hole.1..pos.1 {
                            row[i - 1] = row[i];
                        }
                    }
                    _ => return false,
                }
                row[pos.1] = None;
            }
            (_, Ordering::Equal) => {
                match row_cmp {
                    Ordering::Less => {
                        for i in (pos.0 + 1..hole.0 + 1).rev() {
                            tiles[i + 1][pos.1] = tiles[i][pos.1]
                        }
                    }
                    Ordering::Greater => {
                        for i in hole.0..pos.0 {
                            tiles[i - 1][pos.1] = tiles[i][pos.1]
                        }
                    }
                    _ => return false,
                }
                tiles[pos.0][pos.1] = None;
            }
            _ => return false,
        }

        true
    }
}

#[derive(Clone, Copy)]
struct Tile {
    idx: usize,
    color: Color,
}

impl From<Tile> for Color {
    fn from(value: Tile) -> Self {
        value.color
    }
}

/// Renders the home page of your application.
#[component]
fn HomePage(cx: Scope) -> impl IntoView {
    // Creates a reactive value to update the button
    let (count, set_count) = create_signal(cx, 0);
    let on_click = move |_| set_count.update(|count| *count += 1);

    // using Form is a workaround for a redirecting button
    view! { cx,
        <h1>"Welcome to Leptos!"</h1>
        <button on:click=on_click>"Click Me: " {count}</button>
        <Form method="GET" action="/game">
            <button>"Play"</button>
        </Form>
    }

    // #[cfg(not(feature = "ssr"))]
    // let count = {
    //     use futures::StreamExt;
    //     use gloo_net::websocket::Message;

    //     let ws = gloo_net::websocket::futures::WebSocket::open("ws://localhost:3000/connect")
    //         .expect("could not connect");
    //     create_signal_from_stream(
    //         cx,
    //         ws.map(|msg| match msg {
    //             Ok(msg) => {
    //                 let Message::Binary(s) = msg else { panic!("expected binary") };
    //                 bincode::deserialize(&s).unwrap()
    //             }
    //             Err(_) => 0,
    //         }),
    //     )
    // };

    // #[cfg(feature = "ssr")]
    // let (count, _) = create_signal(cx, None::<i32>);

    // view! { cx,
    //     <h1>"Welcome to Leptos!"</h1>
    //     <h2>{move || count.get().map(|c| c.to_string()).unwrap_or("None".into())}</h2>
    // }
}
