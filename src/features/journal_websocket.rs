use crate::features::journal;
use actix::prelude::*;
use actix::{self, Actor, AsyncContext, Handler, StreamHandler};
use actix_web_actors::ws;
use futures::channel::mpsc::Receiver;
use tracing::*;

use std::sync::{Arc, Mutex};

pub struct StringMessage(String);

impl actix::Message for StringMessage {
    type Result = ();
}

pub struct WebsocketActorContent {
    pub actor: Addr<WebsocketActor>,
}

#[derive(Default)]
pub struct WebsocketManager {
    pub clients: Vec<WebsocketActorContent>,
}

lazy_static! {
    static ref SYSTEM: Arc<Mutex<WebsocketManager>> =
        Arc::new(Mutex::new(WebsocketManager::default()));
}

pub fn manager() -> Arc<Mutex<WebsocketManager>> {
    SYSTEM.clone()
}

pub fn new_websocket() -> WebsocketActor {
    WebsocketActor::new(SYSTEM.clone())
}

pub struct WebsocketActor {
    server: Arc<Mutex<WebsocketManager>>,
    receiver: Option<Receiver<String>>,
}

impl WebsocketActor {
    pub fn new(server: Arc<Mutex<WebsocketManager>>) -> Self {
        Self {
            server,
            receiver: Some(journal::ask_for_client()),
        }
    }
}

impl Handler<StringMessage> for WebsocketActor {
    type Result = ();

    fn handle(&mut self, message: StringMessage, context: &mut Self::Context) {
        context.text(message.0);
    }
}

impl Actor for WebsocketActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Starting journal websocket");
        ctx.add_stream(self.receiver.take().unwrap());
    }
}

impl StreamHandler<String> for WebsocketActor {
    fn handle(&mut self, data: String, ctx: &mut Self::Context) {
        ctx.text(data)
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebsocketActor {
    fn finished(&mut self, ctx: &mut Self::Context) {
        debug!("Finishing journal websocket, removing client");
        self.server
            .lock()
            .unwrap()
            .clients
            .retain(|x| x.actor != ctx.address());
    }

    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Text(_)) => {
                ctx.text("{\"error\":\"Websocket does not support inputs.\"}");
            }
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            _ => (),
        }
    }
}
