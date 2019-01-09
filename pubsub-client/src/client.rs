use crate::request::PubSubRequest;
use log::*;
use serde_json::json;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;
use std::{error, fmt, thread};
use ws::Error as WSError;
use ws::ErrorKind as WSErrorKind;
use ws::Sender as WSSender;
use ws::{connect, CloseCode, Handler, Handshake, Message};

pub fn start_pubsub<T>(
    ws_addr: String,
    method: PubSubRequest,
    param: &T,
) -> Result<PubSubThread, Box<dyn error::Error>>
where
    T: fmt::Display,
{
    let (ws_mpsc_sender, ws_mpsc_receiver) = channel();

    let client = thread::spawn(move || {
        info!("Connecting to {}", ws_addr);
        connect(ws_addr, move |sender| Client {
            ws_out: sender,
            thread_out: ws_mpsc_sender.clone(),
        })
        .unwrap();
    });

    let ws_sender = if let Event::Connect(s) = ws_mpsc_receiver.recv()? {
        s
    } else {
        return Err(PubSubError::ConnectionFailed)?;
    };

    info!("Connected to PubSub websocket");

    info!("Sending PubSub subscription request");
    let params = json!([format!("{}", param)]);
    let request_json = method.build_request_json(1, Some(params));
    let req = serde_json::to_string(&request_json).unwrap();

    info!("sending: '{}'", req);
    ws_sender.send(req)?;

    let subscription_num = if let Event::Message(m) = ws_mpsc_receiver.recv()? {
        info!("Recieved: {}", m);
        let v: serde_json::Value = serde_json::from_str(&m.to_string())?;
        if let Some(res) = v["result"].as_u64() {
            res
        } else {
            return Err(PubSubError::SubscriptionFailed)?;
        }
    } else {
        return Err(PubSubError::ConnectionDropped(
            None,
            "Connection dropped while subscribing to pubsub".to_string(),
        ))?;
    };

    info!(
        "Subscribed to PubSub with subscription number {}",
        subscription_num
    );

    Ok(PubSubThread {
        sender: ws_sender,
        receiver: ws_mpsc_receiver,
        handle: client,
        subscription_num,
    })
}

#[derive(Debug)]
pub struct PubSubThread {
    pub handle: JoinHandle<()>,
    pub sender: WSSender,
    pub receiver: Receiver<Event>,
    pub subscription_num: u64,
}

struct Client {
    ws_out: WSSender,
    thread_out: Sender<Event>,
}

impl Handler for Client {
    fn on_open(&mut self, _: Handshake) -> Result<(), WSError> {
        self.thread_out
            .send(Event::Connect(self.ws_out.clone()))
            .map_err(|err| {
                WSError::new(
                    WSErrorKind::Internal,
                    format!("Unable to communicate between threads: {:?}.", err),
                )
            })
    }

    fn on_message(&mut self, msg: Message) -> Result<(), WSError> {
        match self.thread_out.send(Event::Message(msg)) {
            Ok(_) => Ok(()),
            Err(e) => Err(WSError::new(
                WSErrorKind::Custom(Box::new(e)),
                "Couldn't pass message to parent",
            )),
        }
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        if let Err(e) = self.thread_out.send(Event::Disconnect(code, reason.into())) {
            info!("{:?}", e);
        }
    }

    fn on_error(&mut self, e: ws::Error) {
        error!("WS Error: {:?}", e)
    }
}

pub enum Event {
    Connect(WSSender),
    Disconnect(CloseCode, String),
    Message(Message),
}

#[derive(Debug)]
pub enum PubSubError {
    ConnectionFailed,
    ConnectionDropped(Option<CloseCode>, String),
    SubscriptionFailed,
    DoubleConnect,
}

impl error::Error for PubSubError {}

impl fmt::Display for PubSubError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PubSubError::ConnectionFailed => {
                write!(f, "The PubSub connection could not be established")
            }
            PubSubError::ConnectionDropped(cc, m) => match cc {
                Some(c) => write!(f, "Conection dropped with code {:?} and message {}", c, m),
                None => write!(f, "{}", m),
            },
            PubSubError::SubscriptionFailed => write!(f, "The PubSub subscription failed"),
            PubSubError::DoubleConnect => write!(f, "Recieved a second WS connection"),
        }
    }
}
