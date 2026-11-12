//! Live connections to a test node served on a real local port.

use std::net::SocketAddr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use trackflow_server::router;

use super::{HOST, Node};

/// How long a test waits for a message it expects.
const WAIT: Duration = Duration::from_secs(5);

impl Node {
    /// Serves the router on a free local port, as the binary does.
    pub async fn serve(&self) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = router(self.state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        address
    }
}

/// Opens a live connection as a page of this server would: `credential` is a session cookie
/// value or a bearer token (`tfp_…`, sent without `Origin` as a script would).
pub async fn connect(address: SocketAddr, credential: Option<&str>) -> Live {
    let origin = format!("https://{HOST}");
    let origin = credential
        .is_none_or(|c| !c.starts_with("tfp_"))
        .then_some(origin.as_str());
    try_connect(address, credential, origin).await.unwrap()
}

pub async fn try_connect(
    address: SocketAddr,
    credential: Option<&str>,
    origin: Option<&str>,
) -> Result<Live, tungstenite::Error> {
    let mut request = format!("ws://{address}/api/v1/live")
        .into_client_request()
        .unwrap();
    let headers = request.headers_mut();
    headers.insert("host", HeaderValue::from_static(HOST));
    if let Some(origin) = origin {
        headers.insert("origin", origin.parse().unwrap());
    }
    match credential {
        Some(token) if token.starts_with("tfp_") => {
            headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
        }
        Some(cookie) => {
            headers.insert(
                "cookie",
                format!("trackflow_session={cookie}").parse().unwrap(),
            );
        }
        None => {}
    }
    let (socket, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(Live(socket))
}

pub struct Live(pub WebSocketStream<MaybeTlsStream<TcpStream>>);

impl Live {
    pub async fn send(&mut self, message: Value) {
        self.send_text(&message.to_string()).await;
    }

    pub async fn send_text(&mut self, text: &str) {
        self.0.send(Message::Text(text.into())).await.unwrap();
    }

    /// Subscribes and answers the `subscribed` message; messages before it are returned too.
    pub async fn subscribe(&mut self, topics: &[&str], since: Option<i64>) -> Vec<Value> {
        self.send(json!({ "type": "subscribe", "topics": topics, "since": since }))
            .await;
        let mut got = Vec::new();
        loop {
            let message = self.next().await;
            let done = message["type"] == "subscribed";
            got.push(message);
            if done {
                return got;
            }
        }
    }

    /// The next message; fails after a few seconds.
    pub async fn next(&mut self) -> Value {
        self.within(WAIT).await.expect("a live message")
    }

    /// The next message, if one comes in time.
    pub async fn within(&mut self, wait: Duration) -> Option<Value> {
        loop {
            let frame = tokio::time::timeout(wait, self.0.next()).await.ok()?;
            match frame {
                Some(Ok(Message::Text(text))) => return Some(serde_json::from_str(&text).unwrap()),
                Some(Ok(Message::Close(frame))) => panic!("closed: {frame:?}"),
                Some(Ok(_)) => {}
                Some(Err(error)) => panic!("live connection failed: {error}"),
                None => panic!("live connection ended"),
            }
        }
    }

    /// Messages until the answer to a `ping`: whatever was queued before it.
    pub async fn drain(&mut self) -> Vec<Value> {
        self.send(json!({ "type": "ping" })).await;
        let mut got = Vec::new();
        loop {
            let message = self.next().await;
            if message["type"] == "pong" {
                return got;
            }
            got.push(message);
        }
    }

    /// Nothing was queued for this connection.
    pub async fn assert_quiet(&mut self) {
        let got = self.drain().await;
        assert!(got.is_empty(), "unexpected messages: {got:?}");
    }

    /// Waits for the server's close frame; answers its code.
    pub async fn closed(&mut self) -> u16 {
        loop {
            match tokio::time::timeout(WAIT, self.0.next()).await {
                Ok(Some(Ok(Message::Close(Some(frame))))) => return u16::from(frame.code),
                Ok(Some(Ok(Message::Close(None)))) => panic!("closed without a code"),
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(error))) => panic!("live connection failed: {error}"),
                Ok(None) => panic!("ended without a close frame"),
                Err(_) => panic!("not closed in time"),
            }
        }
    }
}
