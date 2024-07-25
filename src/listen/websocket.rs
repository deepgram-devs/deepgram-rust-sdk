// TODO: Remove this lint
// Currently not documented because interface of this module is still changing
#![allow(missing_docs)]

//! Types used for live audio transcription.
//!
//! See the [Deepgram API Reference][api] for more info.
//!
//! [api]: https://developers.deepgram.com/api-reference/#transcription-streaming

use crate::common::stream_response::StreamResponse;
use serde_urlencoded;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures::channel::mpsc::{self, Receiver};
use futures::stream::StreamExt;
use futures::{SinkExt, Stream};
use http::Request;
use pin_project::pin_project;
use tokio::fs::File;
use tokio::sync::Mutex;
use tokio::time;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::io::ReaderStream;
use tungstenite::handshake::client;
use url::Url;

use crate::common::options::{Encoding, Endpointing, Options, SerializableOptions};
use crate::{Deepgram, DeepgramError, Result, Transcription};

static LIVE_LISTEN_URL_PATH: &str = "v1/listen";

#[derive(Debug)]
pub struct StreamRequestBuilder<'a> {
    config: &'a Deepgram,
    options: Option<&'a Options>,
    encoding: Option<Encoding>,
    sample_rate: Option<u32>,
    channels: Option<u16>,
    endpointing: Option<Endpointing>,
    utterance_end_ms: Option<u16>,
    interim_results: Option<bool>,
    no_delay: Option<bool>,
    vad_events: Option<bool>,
    stream_url: Url,
    keep_alive: Option<bool>,
}

#[pin_project]
struct FileChunker {
    chunk_size: usize,
    buf: BytesMut,
    #[pin]
    file: ReaderStream<File>,
}

impl Transcription<'_> {
    pub fn stream_request(&self) -> StreamRequestBuilder<'_> {
        self.stream_request_with_options(None)
    }

    pub fn stream_request_with_options<'a>(
        &'a self,
        options: Option<&'a Options>,
    ) -> StreamRequestBuilder<'a> {
        StreamRequestBuilder {
            config: self.0,
            options,
            encoding: None,
            sample_rate: None,
            channels: None,
            endpointing: None,
            utterance_end_ms: None,
            interim_results: None,
            no_delay: None,
            vad_events: None,
            stream_url: self.listen_stream_url(),
            keep_alive: None,
        }
    }

    fn listen_stream_url(&self) -> Url {
        let mut url = self.0.base_url.join(LIVE_LISTEN_URL_PATH).unwrap();
        match url.scheme() {
            "http" | "ws" => url.set_scheme("ws").unwrap(),
            "https" | "wss" => url.set_scheme("wss").unwrap(),
            _ => panic!("base_url must have a scheme of http, https, ws, or wss"),
        }
        url
    }
}

impl FileChunker {
    fn new(file: File, chunk_size: usize) -> Self {
        FileChunker {
            chunk_size,
            buf: BytesMut::with_capacity(2 * chunk_size),
            file: ReaderStream::new(file),
        }
    }
}

impl Stream for FileChunker {
    type Item = Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        while this.buf.len() < *this.chunk_size {
            match Pin::new(&mut this.file).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(next) => match next.transpose() {
                    Err(e) => return Poll::Ready(Some(Err(DeepgramError::from(e)))),
                    Ok(None) => {
                        if this.buf.is_empty() {
                            return Poll::Ready(None);
                        } else {
                            return Poll::Ready(Some(Ok(this
                                .buf
                                .split_to(this.buf.len())
                                .freeze())));
                        }
                    }
                    Ok(Some(next)) => {
                        this.buf.extend_from_slice(&next);
                    }
                },
            }
        }

        Poll::Ready(Some(Ok(this.buf.split_to(*this.chunk_size).freeze())))
    }
}

impl<'a> StreamRequestBuilder<'a> {
    pub fn keep_alive(mut self) -> Self {
        self.keep_alive = Some(true);

        self
    }

    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = Some(encoding);

        self
    }

    pub fn sample_rate(mut self, sample_rate: u32) -> Self {
        self.sample_rate = Some(sample_rate);

        self
    }

    pub fn channels(mut self, channels: u16) -> Self {
        self.channels = Some(channels);

        self
    }

    pub fn endpointing(mut self, endpointing: Endpointing) -> Self {
        self.endpointing = Some(endpointing);

        self
    }

    pub fn utterance_end_ms(mut self, utterance_end_ms: u16) -> Self {
        self.utterance_end_ms = Some(utterance_end_ms);

        self
    }

    pub fn interim_results(mut self, interim_results: bool) -> Self {
        self.interim_results = Some(interim_results);

        self
    }

    pub fn no_delay(mut self, no_delay: bool) -> Self {
        self.no_delay = Some(no_delay);

        self
    }

    pub fn vad_events(mut self, vad_events: bool) -> Self {
        self.vad_events = Some(vad_events);

        self
    }
}

#[derive(Debug)]
pub struct StreamRequest<'a, S, E> {
    stream: S,
    builder: StreamRequestBuilder<'a>,
    _err: PhantomData<E>,
}

impl<'a> StreamRequestBuilder<'a> {
    pub async fn file(
        self,
        filename: impl AsRef<Path>,
        frame_size: usize,
        frame_delay: Duration,
    ) -> Result<
        StreamRequest<'a, Receiver<Result<Bytes, DeepgramError>>, DeepgramError>,
        DeepgramError,
    > {
        let file = File::open(filename).await?;
        let mut chunker = FileChunker::new(file, frame_size);
        let (mut tx, rx) = mpsc::channel(1);
        let task = async move {
            while let Some(frame) = chunker.next().await {
                tokio::time::sleep(frame_delay).await;
                // This unwrap() is safe because application logic dictates that the Receiver won't
                // be dropped before the Sender.
                tx.send(frame).await.unwrap();
            }
        };
        tokio::spawn(task);
        Ok(self.stream(rx))
    }
    pub fn stream<S, E>(self, stream: S) -> StreamRequest<'a, S, E> {
        StreamRequest {
            stream,
            builder: self,
            _err: PhantomData,
        }
    }
}

fn options_to_query_string(options: &Options) -> String {
    let serialized_options = SerializableOptions::from(options);
    serde_urlencoded::to_string(serialized_options).unwrap_or_default()
}

impl<S, E> StreamRequest<'_, S, E>
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Send + Unpin + 'static,
    E: Error + Debug + Send + Unpin + 'static,
{
    pub async fn start(self) -> Result<Receiver<Result<StreamResponse>>> {
        let mut url = self.builder.stream_url;
        {
            let mut pairs = url.query_pairs_mut();

            // Add standard pre-recorded options
            if let Some(options) = &self.builder.options {
                let query_string = options_to_query_string(options);
                let query_pairs: Vec<(Cow<str>, Cow<str>)> = query_string
                    .split('&')
                    .map(|s| {
                        let mut split = s.splitn(2, '=');
                        (
                            Cow::from(split.next().unwrap_or_default()),
                            Cow::from(split.next().unwrap_or_default()),
                        )
                    })
                    .collect();

                for (key, value) in query_pairs {
                    pairs.append_pair(&key, &value);
                }
            }
            if let Some(encoding) = &self.builder.encoding {
                pairs.append_pair("encoding", encoding.as_str());
            }
            if let Some(sample_rate) = self.builder.sample_rate {
                pairs.append_pair("sample_rate", &sample_rate.to_string());
            }
            if let Some(channels) = self.builder.channels {
                pairs.append_pair("channels", &channels.to_string());
            }
            if let Some(endpointing) = self.builder.endpointing {
                pairs.append_pair("endpointing", &endpointing.to_str());
            }
            if let Some(utterance_end_ms) = self.builder.utterance_end_ms {
                pairs.append_pair("utterance_end_ms", &utterance_end_ms.to_string());
            }
            if let Some(interim_results) = self.builder.interim_results {
                pairs.append_pair("interim_results", &interim_results.to_string());
            }
            if let Some(no_delay) = self.builder.no_delay {
                pairs.append_pair("no_delay", &no_delay.to_string());
            }
            if let Some(vad_events) = self.builder.vad_events {
                pairs.append_pair("vad_events", &vad_events.to_string());
            }
        }

        let mut source = self
            .stream
            .map(|res| res.map(|bytes| Message::binary(Vec::from(bytes.as_ref()))));

        let request = {
            let builder = Request::builder()
                .method("GET")
                .uri(url.to_string())
                .header("sec-websocket-key", client::generate_key())
                .header("host", "api.deepgram.com")
                .header("connection", "upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13");

            let builder = if let Some(api_key) = self.builder.config.api_key.as_deref() {
                builder.header("authorization", format!("token {}", api_key))
            } else {
                builder
            };
            builder.body(())?
        };
        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
        let (write, mut read) = ws_stream.split();
        let write = Arc::new(Mutex::new(write));
        let (mut tx, rx) = mpsc::channel::<Result<StreamResponse>>(1);

        // Spawn the keep-alive task
        if self.builder.keep_alive.unwrap_or(false) {
            {
                let write_clone = Arc::clone(&write);
                tokio::spawn(async move {
                    let mut interval = time::interval(Duration::from_secs(10));
                    loop {
                        interval.tick().await;
                        let keep_alive_message =
                            Message::Text("{\"type\": \"KeepAlive\"}".to_string());
                        let mut write = write_clone.lock().await;
                        if let Err(e) = write.send(keep_alive_message).await {
                            eprintln!("Error Sending Keep Alive: {:?}", e);
                            break;
                        }
                    }
                })
            };
        }

        let write_clone = Arc::clone(&write);
        let send_task = async move {
            while let Some(frame) = source.next().await {
                match frame {
                    Ok(frame) => {
                        let mut write = write_clone.lock().await;
                        if let Err(e) = write.send(frame).await {
                            println!("Error sending frame: {:?}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        println!("Error receiving from source: {:?}", e);
                        break;
                    }
                }
            }

            let mut write = write_clone.lock().await;
            if let Err(e) = write.send(Message::binary([])).await {
                println!("Error sending final frame: {:?}", e);
            }
        };

        let recv_task = async move {
            loop {
                match read.next().await {
                    None => break,
                    Some(Ok(msg)) => {
                        if let Message::Text(txt) = msg {
                            let resp = serde_json::from_str(&txt).map_err(DeepgramError::from);
                            tx.send(resp)
                                .await
                                // This unwrap is probably not safe.
                                .unwrap();
                        }
                    }
                    Some(e) => {
                        let _ = dbg!(e);
                        break;
                    }
                }
            }
        };

        tokio::spawn(async move {
            tokio::join!(send_task, recv_task);
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_stream_url() {
        let dg = crate::Deepgram::new("token");
        assert_eq!(
            dg.transcription().listen_stream_url().to_string(),
            "wss://api.deepgram.com/v1/listen",
        );
    }

    #[test]
    fn test_stream_url_custom_host() {
        let dg = crate::Deepgram::with_base_url_and_api_key("http://localhost:8080", "token");
        assert_eq!(
            dg.transcription().listen_stream_url().to_string(),
            "ws://localhost:8080/v1/listen",
        );
    }
}
