use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

#[cfg(unix)]
type IpcStream = tokio::net::UnixStream;

#[cfg(windows)]
type IpcStream = tokio::net::windows::named_pipe::NamedPipeClient;

struct Request {
    command: Value,
    reply: oneshot::Sender<Result<Value>>,
}

pub enum MpvNotification {
    Event(Value),
    Disconnected(String),
}

enum Incoming {
    Reply {
        request_id: i64,
        result: Result<Value>,
    },
    Event(Value),
    Unknown,
}

impl Incoming {
    fn new(message: Value) -> Self {
        if message.get("event").and_then(Value::as_str).is_some() {
            Self::Event(message)
        } else if let Some(request_id) = message.get("request_id").and_then(Value::as_i64) {
            let result = match message.get("error").and_then(Value::as_str) {
                Some("success") => Ok(message.get("data").cloned().unwrap_or(Value::Null)),
                Some(error) => Err(anyhow!("mpv command failed: {error}")),
                None => Err(anyhow!("malformed mpv response: {message}")),
            };

            Self::Reply { request_id, result }
        } else {
            Self::Unknown
        }
    }
}

pub struct MpvIpc {
    request_tx: mpsc::Sender<Request>,
}

impl MpvIpc {
    pub async fn connect(
        endpoint: &Path,
    ) -> Result<(Self, mpsc::UnboundedReceiver<MpvNotification>)> {
        let stream = connect_stream(endpoint)
            .await
            .with_context(|| format!("failed to connect to mpv at {}", endpoint.display()))?;

        let (request_tx, request_rx) = mpsc::channel(32);
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            if let Err(error) = run_connection(stream, request_rx, message_tx.clone()).await {
                let _ = message_tx.send(MpvNotification::Disconnected(error.to_string()));
            }
        });

        Ok((Self { request_tx }, message_rx))
    }

    pub async fn call(&self, command: Value) -> Result<Value> {
        if !command.is_array() {
            bail!("mpv command must be a JSON array");
        }

        let (reply_tx, reply_rx) = oneshot::channel();

        self.request_tx
            .send(Request {
                command,
                reply: reply_tx,
            })
            .await
            .context("mpv IPC task stopped")?;

        reply_rx.await.context("mpv IPC connection closed")?
    }

    pub async fn load_file(&self, file: &str, start: Option<u64>) -> Result<i64> {
        let mut command = serde_json::json!(["loadfile", file, "replace"]);

        if let Some(position) = start {
            let args = command
                .as_array_mut()
                .context("loadfile command was not an array")?;

            args.push(serde_json::json!(-1));
            args.push(serde_json::json!({ "start": position.to_string() }));
        }

        let data = self.call(command).await?;

        data.get("playlist_entry_id")
            .and_then(Value::as_i64)
            .context("mpv loadfile response omitted playlist_entry_id")
    }

    pub async fn time_pos(&self) -> Result<Option<u64>> {
        self.call(serde_json::json!(["get_property", "time-pos"]))
            .await
            .map(|value| value.as_f64().map(|secs| secs.floor() as u64))
    }

    pub async fn observe_property(&self, property: &str) -> Result<()> {
        static PROPERTY_ID: AtomicU16 = AtomicU16::new(1);

        self.call(serde_json::json!([
            "observe_property",
            PROPERTY_ID.fetch_add(1, Ordering::Relaxed),
            property
        ]))
        .await?;

        Ok(())
    }
}

#[cfg(unix)]
async fn connect_stream(endpoint: &Path) -> std::io::Result<IpcStream> {
    tokio::net::UnixStream::connect(endpoint).await
}

#[cfg(windows)]
async fn connect_stream(endpoint: &Path) -> std::io::Result<IpcStream> {
    use tokio::net::windows::named_pipe::ClientOptions;

    ClientOptions::new().open(endpoint)
}

async fn run_connection(
    stream: IpcStream,
    mut request_rx: mpsc::Receiver<Request>,
    event_tx: mpsc::UnboundedSender<MpvNotification>,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();

    let mut next_request_id: i64 = 1;
    let mut pending = HashMap::new();

    loop {
        tokio::select! {
            request = request_rx.recv() => {
                let Some(request) = request else {
                    return Ok(());
                };

                let request_id = next_request_id;
                next_request_id = next_request_id.wrapping_add(1);

                let wire_message = serde_json::json!({
                    "command": request.command,
                    "request_id": request_id,
                });

                let mut encoded = serde_json::to_vec(&wire_message)?;
                encoded.push(b'\n');

                writer.write_all(&encoded).await?;
                pending.insert(request_id, request.reply);
            }

            result = reader.read_until(b'\n', &mut line) => {
                if result? == 0 {
                    bail!("mpv closed the IPC connection");
                }

                let message: Value = serde_json::from_slice(&line)?;
                line.clear();

                match Incoming::new(message) {
                    Incoming::Reply{ request_id, result} => {
                        if let Some(reply_tx) = pending.remove(&request_id) {
                            let _ = reply_tx.send(result);
                        }
                    }
                    Incoming::Event(event) => {
                        let _ = event_tx.send(MpvNotification::Event(event));
                    }
                    Incoming::Unknown => (),
                }
            }
        }
    }
}
