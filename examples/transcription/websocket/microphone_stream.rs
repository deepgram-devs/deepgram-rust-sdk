use std::env;
use std::thread;

use bytes::{BufMut, Bytes, BytesMut};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use crossbeam::channel::RecvError;
use deepgram::common::options::Encoding;
use futures::channel::mpsc::{self, Receiver as FuturesReceiver};
use futures::stream::StreamExt;
use futures::SinkExt;

use deepgram::{Deepgram, DeepgramError};

fn microphone_as_stream() -> FuturesReceiver<Result<Bytes, RecvError>> {
    let (sync_tx, sync_rx) = crossbeam::channel::unbounded();
    let (mut async_tx, async_rx) = mpsc::channel(1);

    thread::spawn(move || {
        let host = cpal::default_host();
        let device = host.default_input_device().unwrap();

        // let config = device.supported_input_configs().unwrap();
        // for config in config {
        //     dbg!(&config);
        // }

        let config = device.default_input_config().unwrap();

        // dbg!(&config);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        let mut bytes = BytesMut::with_capacity(data.len() * 2);
                        for sample in data {
                            bytes.put_i16_le(sample.to_i16());
                        }
                        sync_tx.send(bytes.freeze()).unwrap();
                    },
                    |_| panic!(),
                )
                .unwrap(),
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        let mut bytes = BytesMut::with_capacity(data.len() * 2);
                        for sample in data {
                            bytes.put_i16_le(*sample);
                        }
                        sync_tx.send(bytes.freeze()).unwrap();
                    },
                    |_| panic!(),
                )
                .unwrap(),
            cpal::SampleFormat::U16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &_| {
                        let mut bytes = BytesMut::with_capacity(data.len() * 2);
                        for sample in data {
                            bytes.put_i16_le(sample.to_i16());
                        }
                        sync_tx.send(bytes.freeze()).unwrap();
                    },
                    |_| panic!(),
                )
                .unwrap(),
        };

        stream.play().unwrap();

        loop {
            thread::park();
        }
    });

    tokio::spawn(async move {
        loop {
            let data = sync_rx.recv();
            async_tx.send(data).await.unwrap();
        }
    });

    async_rx
}

#[tokio::main]
async fn main() -> Result<(), DeepgramError> {
    let dg = Deepgram::new(env::var("DEEPGRAM_API_KEY").unwrap()).unwrap();

    let mut results = dg
        .transcription()
        .stream_request()
        .keep_alive()
        .encoding(Encoding::Linear16)
        // TODO Specific to my machine, not general enough example.
        .sample_rate(44100)
        // TODO Specific to my machine, not general enough example.
        .channels(2)
        .stream(microphone_as_stream())
        .await?;

    while let Some(result) = results.next().await {
        println!("got: {:?}", result);
    }

    Ok(())
}
