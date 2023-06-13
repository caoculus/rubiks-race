use cfg_if::cfg_if;
use tokio::sync::mpsc::UnboundedReceiver;

cfg_if!(
    if #[cfg(feature = "ssr")] {
        use crate::error_template::AppError;
        use axum::{
            extract::{ws::{WebSocket, Message}, WebSocketUpgrade},
            response::Response,
        };
        use tokio::select;
        use leptos::log;

        pub async fn lobby_loop(mut rx: UnboundedReceiver<WebSocket>) {
            todo!()
            // let mut pending = None::<WebSocket>;

            // loop {
            //     let wait_msg = async {
            //         if let Some(ws) = pending.as_mut() {
            //             ws.recv().await
            //         } else {
            //             std::future::pending().await
            //         }
            //     };

            //     select! {
            //         _ = wait_msg => {
            //             // the message is always unexpected: just drop
            //             log!("got unexpected message");
            //             pending = None;
            //         }
            //         req = rx.recv() => {
            //             let Some(ConnectRequest { ws }) = req else { log!("lobby receiver dropped"); break; };
            //             if let Some(ws2) = pending.take() {
            //                 tokio::spawn(game_loop(GameStart { wss: [ws, ws2] }));
            //             }
            //         }
            //     }
            // }
        }

        struct GameStart {
            wss: [WebSocket; 2],
        }

        async fn game_loop(start: GameStart) {
            todo!()
        }

        pub async fn connect(ws: WebSocketUpgrade) -> Result<Response, AppError> {
            Ok(ws.on_upgrade(|ws| async move { run(ws).await }))
        }

        async fn run(mut ws: WebSocket) {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            let mut count = 0i32;

            loop {
                interval.tick().await;

                let msg = bincode::serialize(&count).unwrap();
                if ws.send(Message::Binary(msg)).await.is_err() {
                    break;
                }

                count += 1;
            }
        }
    }
);
