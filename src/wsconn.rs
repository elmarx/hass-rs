use crate::command::Command;
use crate::errors::{HassError, HassResult};
use crate::messages::Response;
use crate::runtime::{connect_async, task, WebSocket};

use async_tungstenite::tungstenite::Message as TungsteniteMessage;
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::lock::Mutex;
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use url;
use uuid::Uuid;
// #[derive(Debug)]
// pub enum Cmd {
//     Msg((Uuid, Sender<HassResult<Response>>, Vec<u8>)),
//     Pong(Vec<u8>),
//     Shutdown,
// }
#[derive(Debug)]
pub struct WsConn {
    last_sequence: Arc<AtomicU64>,
    pub(crate) to_gateway: Sender<Command>,
    //below will be used to listen for events, see panda, gateway mod
    //pub(crate) from_gateway: UnboundedReceiver<Event>,
}

impl WsConn {
    pub async fn connect(url: url::Url) -> HassResult<WsConn> {
        let wsclient = connect_async(url).await.expect("Can't connect to gateway");
        let (sink, stream) = wsclient.split();
        let (to_gateway, from_client) = channel::<Command>(20);

        let last_sequence = Arc::new(AtomicU64::default());
        let last_sequence_clone_sender = Arc::clone(&last_sequence);
        let last_sequence_clone_receiver = Arc::clone(&last_sequence);

        let requests: Arc<Mutex<HashMap<Uuid, Sender<HassResult<Response>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        sender_loop(
            last_sequence_clone_sender,
            sink,
            requests.clone(),
            from_client,
        );
        receiver_loop(
            last_sequence_clone_receiver,
            stream,
            requests.clone(),
            to_gateway.clone(),
        );

        // Receive Hello event from the gatewat
        // let event = from_gateway.next().await.ok_or_else(|| PandaError::ConnectionClosed)?;

        // let heartbeat_interval = match event {
        //     Event::Hello(v) => v,
        //     _ => return Err(PandaError::UnknownPayloadReceived.into()),
        // };

        Ok(WsConn {
            last_sequence,
            to_gateway,
        })
    }

    // async fn run(
    //     &mut self,
    //     id: u64,
    //     payload: String,
    // ) -> HassResult<TungsteniteMessage> {
    //     let (sender, mut receiver) = channel(1);

    //     self.to_gateway
    //         .send(Command::Msg(id, payload))
    //         .await?;

    //     receiver
    //         .next()
    //         .await
    //         .expect("It should contain the response")
    // }
}

fn sender_loop(
    last_sequence: Arc<AtomicU64>,
    mut sink: SplitSink<WebSocket, TungsteniteMessage>,
    requests: Arc<Mutex<HashMap<Uuid, Sender<HassResult<Response>>>>>,
    mut from_client: Receiver<Command>,
) {
    task::spawn(async move {
        loop {
            match from_client.next().await {
                Some(item) => match item {
                    Command::Auth(auth) => {
                        // Get the last sequence
                        // let seq = match last_sequence.load(Ordering::Relaxed) {
                        //         0 => None,
                        //          v => Some(v),
                        // };

                        // Transform command to TungsteniteMessage
                        let cmd = Command::Auth(auth).to_tungstenite_message(None);

                        // Send command to gateway
                        // NOT GOOD as it is not returned
                        sink.send(cmd)
                            .await
                            .map_err(|_| HassError::ConnectionClosed)
                            .unwrap();

                        // Send command to gateway
                        // if let Err(e) = sink.send(TungsteniteMessage::Text(item)).await {
                        //     let mut sender = guard.remove(&msg.0).unwrap();
                        //     sender
                        //         .send(Err(HassError::from(e)))
                        //         .await
                        //         .expect("Failed to send error");
                        // };
                    }
                    // Command::Msg(msg) => {
                    //     let mut guard = requests.lock().await;
                    //     guard.insert(msg.0, msg.1);
                    //     if let Err(e) = sink.send(TungsteniteMessage::Binary(msg.2)).await {
                    //         let mut sender = guard.remove(&msg.0).unwrap();
                    //         sender
                    //             .send(Err(HassError::from(e)))
                    //             .await
                    //             .expect("Failed to send error");
                    //     }
                    //     drop(guard);
                    // }
                    // Command::Pong(data) => {
                    //     sink.send(TungsteniteMessage::Pong(data))
                    //         .await
                    //         .expect("Failed to send pong message.");
                    // }
                    // Command::Shutdown => {
                    //     let mut guard = requests.lock().await;
                    //     guard.clear();
                    // }
                    _ => todo!(),
                },
                None => {}
            }
        }
    });
}

fn receiver_loop(
    last_sequence: Arc<AtomicU64>,
    mut stream: SplitStream<WebSocket>,
    requests: Arc<Mutex<HashMap<Uuid, Sender<HassResult<Response>>>>>,
    mut to_gateway: Sender<Command>,
) {
    task::spawn(async move {
        loop {
            match stream.next().await {
                Some(Err(error)) => {
                    let mut guard = requests.lock().await;
                    for s in guard.values_mut() {
                        match s.send(Err(HassError::from(&error))).await {
                            Ok(_r) => {}
                            Err(_e) => {}
                        }
                    }
                    guard.clear();
                }
                Some(Ok(item)) => match item {
                    TungsteniteMessage::Text(data) => {
                        let response: Response = serde_json::from_str(&data)
                            .map_err(|_| HassError::UnknownPayloadReceived)
                            .unwrap();
                        let mut guard = requests.lock().await;
                        if response.status.code != 206 {
                            let item = guard.remove(&response.sequence);
                            drop(guard);
                            if let Some(mut s) = item {
                                match s.send(Ok(response)).await {
                                    Ok(_r) => {}
                                    Err(_e) => {}
                                };
                            }
                        } else {
                            let item = guard.get_mut(&response.sequence);
                            if let Some(s) = item {
                                match s.send(Ok(response)).await {
                                    Ok(_r) => {}
                                    Err(_e) => {}
                                };
                            }
                            drop(guard);
                        }
                    }
                    // TungsteniteMessage::Binary(data) => {
                    //     let response: Response = serde_json::from_slice(&data).unwrap();
                    //     let mut guard = requests.lock().await;
                    //     if response.status.code != 206 {
                    //         let item = guard.remove(&response.sequence);
                    //         drop(guard);
                    //         if let Some(mut s) = item {
                    //             match s.send(Ok(response)).await {
                    //                 Ok(_r) => {}
                    //                 Err(_e) => {}
                    //             };
                    //         }
                    //     } else {
                    //         let item = guard.get_mut(&response.sequence);
                    //         if let Some(s) = item {
                    //             match s.send(Ok(response)).await {
                    //                 Ok(_r) => {}
                    //                 Err(_e) => {}
                    //             };
                    //         }
                    //         drop(guard);
                    //     }
                    // }
                    // TungsteniteMessage::Ping(data) => sender.send(Command::Pong(data)).await.unwrap(),
                    _ => {}
                },
                None => {}
            }
        }
    });
}
