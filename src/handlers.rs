#![cfg(feature = "ssr")]

use crate::{
    error_template::AppError,
    types::{BoardInner, BoardTiles, ClientMessage, Color, GameStart, ServerMessage, Target},
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, WebSocketUpgrade,
    },
    response::Response,
};
use futures::StreamExt;
use leptos::log;
use rand::{distributions::Standard, prelude::Distribution};
use rand::{seq::SliceRandom, Rng};
use strum::EnumCount;
use tokio::{
    select,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

enum GameEvent {
    Message { id: usize, msg: ClientMessage },
    Disconnected { id: usize },
}

pub async fn connect(
    Extension(ws_tx): Extension<UnboundedSender<WebSocket>>,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    Ok(ws.on_upgrade(|ws| async move {
        _ = ws_tx.send(ws);
    }))
}

pub async fn lobby_loop(mut ws_rx: UnboundedReceiver<WebSocket>) {
    loop {
        wait_for_players(&mut ws_rx).await;
    }
}

async fn wait_for_players(ws_rx: &mut UnboundedReceiver<WebSocket>) {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<GameEvent>();
    let mut msg_txs: [Option<UnboundedSender<ServerMessage>>; 2] = std::array::from_fn(|_| None);
    let mut free_ids = vec![0, 1];

    log!("Waiting for players");

    loop {
        select! {
            event = event_rx.recv() => {
                let event = event.expect("event_rx stopped, but event_tx shouldn't be dropped");
                let id = match event {
                    GameEvent::Message { id, msg: ClientMessage::Ping } => {
                        log!("Received ping from {id}");
                        continue;
                    }
                    GameEvent::Message { id, msg: _ } => {
                        log!("Unexpected message from id {id}");
                        id
                    }
                    GameEvent::Disconnected { id } => {
                        id
                    }
                };
                log!("Freeing id {id}");
                msg_txs[id] = None;
                free_ids.push(id);
            }
            ws = ws_rx.recv() => {
                let Some(ws) = ws else { log!("ws_rx stopped"); break; };
                let id = free_ids.pop().expect("no ids left");
                let (msg_tx, msg_rx) = mpsc::unbounded_channel();

                log!("Assigning id {id}");

                tokio::spawn(ws_loop(id, ws, event_tx.clone(), msg_rx));
                msg_txs[id] = Some(msg_tx);

                if !free_ids.is_empty() {
                    continue;
                }

                let full_msg_txs = msg_txs.map(|tx| tx.expect("msg_tx is None"));

                log!("Starting new game");
                tokio::spawn(game_loop(event_rx, full_msg_txs));
                return;
            }
        }
    }
}

async fn game_loop(
    mut event_rx: UnboundedReceiver<GameEvent>,
    mut msg_txs: [UnboundedSender<ServerMessage>; 2],
) {
    log!("Entering game loop");

    let target = generate_target();
    let mut boards = [Board::generate(), Board::generate()];

    for (id, tx) in msg_txs.iter_mut().enumerate() {
        _ = tx.send(ServerMessage::GameStart(GameStart {
            target,
            board: boards[id].0,
            opponent_board: boards[1 - id].0,
        }));
    }

    while let Some(event) = event_rx.recv().await {
        match event {
            GameEvent::Message {
                id,
                msg: ClientMessage::Click { pos },
            } => {
                if pos.0 >= 5 || pos.1 >= 5 {
                    log!("Out of bounds click position: {:?}", pos);
                    break;
                }
                let updated = boards[id].click_tile(pos);
                if !updated {
                    log!("Click position did not move tile: {:?}", pos);
                    break;
                }

                let other_id = 1 - id;
                _ = msg_txs[other_id].send(ServerMessage::OpponentClick { pos });

                if !boards[id].matches_target(&target) {
                    continue;
                }

                // win handling
                _ = msg_txs[id].send(ServerMessage::GameEnd { is_win: true });
                _ = msg_txs[other_id].send(ServerMessage::GameEnd { is_win: false });
                break;
            }
            GameEvent::Message {
                id,
                msg: ClientMessage::Ping,
            } => {
                log!("Received ping from {id}")
            }
            GameEvent::Disconnected { id } => {
                let other_id = 1 - id;
                _ = msg_txs[other_id].send(ServerMessage::OpponentLeft);
                break;
            }
        }
    }

    log!("Exiting game loop");
}

fn generate_target() -> Target {
    let mut target: Target = Default::default();

    'retry: loop {
        let mut counts = [0; Color::COUNT];

        for row in &mut target {
            for slot in row {
                let color = rand::thread_rng().gen::<Color>();
                let count = &mut counts[color as usize];

                // too many of same color
                *count += 1;
                if *count > 4 {
                    continue 'retry;
                }

                *slot = color;
            }
        }

        break;
    }

    target
}

async fn ws_loop(
    id: usize,
    mut ws: WebSocket,
    event_tx: UnboundedSender<GameEvent>,
    mut msg_rx: UnboundedReceiver<ServerMessage>,
) {
    log!("Entering ws_loop");
    loop {
        select! {
            msg = ws.next() => {
                let Some(Ok(msg)) = msg else { break; };
                let msg = match msg {
                    Message::Binary(msg) => msg,
                    Message::Ping(_) => continue,
                    _ => break,
                };
                let Ok(msg) = bincode::deserialize(&msg) else {
                    log!("got invalid message");
                    break;
                };
                if event_tx.send(GameEvent::Message { id, msg }).is_err() {
                    break;
                }
            }
            msg = msg_rx.recv() => {
                let Some(msg) = msg else { break; };
                let msg = bincode::serialize(&msg).expect("failed to serialize");
                if let Err(e) = ws.send(Message::Binary(msg)).await {
                    log!("Error when sending message: {e}");
                    break;
                }
            }
        }
    }
    _ = event_tx.send(GameEvent::Disconnected { id });
    log!("Exiting ws_loop");
}

impl Distribution<Color> for Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Color {
        rng.gen_range(0..Color::COUNT).into()
    }
}

struct Board(BoardInner);

impl Board {
    fn generate() -> Self {
        let mut colors: [Color; 24] = std::array::from_fn(|i| (i / 4).into());
        colors.shuffle(&mut rand::thread_rng());

        let mut colors = colors.into_iter();
        let mut tiles = BoardTiles::default();

        for (i, row) in tiles.iter_mut().enumerate() {
            for (j, slot) in row.iter_mut().enumerate() {
                // we will always leave the center tile empty
                if i == 2 && j == 2 {
                    continue;
                }

                *slot = Some(colors.next().unwrap());
            }
        }

        Board(BoardInner {
            tiles,
            hole: (2, 2),
        })
    }

    fn click_tile(&mut self, pos: (usize, usize)) -> bool {
        use crate::utils::slide;

        let BoardInner { tiles, hole } = &mut self.0;
        let update = |old: (usize, usize), new: (usize, usize)| {
            tiles[new.0][new.1] = tiles[old.0][old.1];
        };

        if !slide(pos, *hole, update) {
            return false;
        }

        *hole = pos;
        true
    }

    fn matches_target(&self, target: &Target) -> bool {
        self.0.matches_target(target)
    }
}
