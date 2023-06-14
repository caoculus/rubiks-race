use cfg_if::cfg_if;

cfg_if!(
    if #[cfg(feature = "ssr")] {
        use crate::{
            error_template::AppError,
            types::{Board, ClientMessage, Color, GameStart, ServerMessage, Target},
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
            let (mut event_tx, mut event_rx) = mpsc::unbounded_channel::<GameEvent>();
            let mut msg_txs: [Option<UnboundedSender<ServerMessage>>; 2] = std::array::from_fn(|_| None);
            let mut free_ids = vec![0, 1];

            loop {
                select! {
                    event = event_rx.recv() => {
                        let event = event.expect("event_rx stopped, but event_tx shouldn't be dropped");
                        let id = match event {
                            GameEvent::Message { id, msg: _ } => {
                                log!("Unexpected message from id {id}");
                                id
                            }
                            GameEvent::Disconnected { id } => {
                                id
                            }
                        };
                        msg_txs[id] = None;
                        free_ids.push(id);
                    }
                    ws = ws_rx.recv() => {
                        let Some(ws) = ws else { log!("ws_rx stopped"); break; };
                        let id = free_ids.pop().expect("no ids left");
                        let (msg_tx, msg_rx) = mpsc::unbounded_channel();

                        tokio::spawn(ws_loop(id, ws, event_tx.clone(), msg_rx));
                        msg_txs[id] = Some(msg_tx);

                        if !free_ids.is_empty() {
                            continue;
                        }

                        let full_msg_txs = std::mem::take(&mut msg_txs).map(|tx| tx.expect("msg_tx is None"));
                        let old_event_rx = event_rx;

                        free_ids = vec![0, 1];
                        (event_tx, event_rx) = mpsc::unbounded_channel();

                        tokio::spawn(game_loop(old_event_rx, full_msg_txs));
                    }
                }
            }
        }

        async fn game_loop(
            mut event_rx: UnboundedReceiver<GameEvent>,
            mut msg_txs: [UnboundedSender<ServerMessage>; 2],
        ) {
            type BoardTiles = [[Option<Color>; 5]; 5];

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

            fn generate_target() -> Target {
                let mut target: Target = Default::default();

                'retry: loop {
                    let mut counts = [0; Color::COUNT];

                    for row in &mut target {
                        for slot in row {
                            let idx = rand::thread_rng().gen_range(0..Color::COUNT);
                            let count = &mut counts[idx];

                            // too many of same color
                            *count += 1;
                            if *count > 4 {
                                continue 'retry;
                            }

                            *slot = idx.into();
                        }
                    }

                    break;
                }

                target
            }

            fn generate_board() -> Board<Color> {
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

                Board {
                    tiles,
                    hole: (2, 2),
                }
            }

            let target = generate_target();
            let mut boards = [generate_board(), generate_board()];

            for (id, tx) in msg_txs.iter_mut().enumerate() {
                _ = tx.send(ServerMessage::GameStart(GameStart {
                    target,
                    board: boards[id].tiles,
                    opponent_board: boards[1 - id].tiles,
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
                    GameEvent::Disconnected { id } => {
                        let other_id = 1 - id;
                        _ = msg_txs[other_id].send(ServerMessage::OpponentLeft);
                        break;
                    }
                }
            }
        }

        async fn ws_loop(
            id: usize,
            mut ws: WebSocket,
            event_tx: UnboundedSender<GameEvent>,
            mut msg_rx: UnboundedReceiver<ServerMessage>,
        ) {
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
        }
    }
);
