#![forbid(unsafe_code)]
#![warn(missing_debug_implementations, missing_docs, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::derive_partial_eq_without_eq)]

//! Official Rust SDK for Deepgram's automated speech recognition APIs.
//!
//! Get started transcribing with a [`Transcription`](transcription::Transcription) object.

use std::io;

use reqwest::{
    header::{HeaderMap, HeaderValue},
    RequestBuilder,
};
use serde::de::DeserializeOwned;
use thiserror::Error;
use url::Url;

pub mod billing;
pub mod invitations;
pub mod keys;
pub mod members;
pub mod projects;
pub mod scopes;
pub mod transcription;
pub mod usage;

mod response;

static DEEPGRAM_BASE_URL: &str = "https://api.deepgram.com";

/// A client for the Deepgram API.
///
/// Make transcriptions requests using [`Deepgram::transcription`].
#[derive(Debug, Clone)]
pub struct Deepgram {
    #[cfg_attr(not(feature = "live"), allow(unused))]
    api_key: String,
    #[cfg_attr(
        all(not(feature = "live"), not(feature = "prerecorded")),
        allow(unused)
    )]
    base_url: Url,
    client: reqwest::Client,
}

/// Errors that may arise from the [`deepgram`](crate) crate.
// TODO sub-errors for the different types?
#[derive(Debug, Error)]
pub enum DeepgramError {
    /// No source was provided to the request builder.
    #[error("No source was provided to the request builder.")]
    NoSource,

    /// The Deepgram API returned an error.
    #[error("The Deepgram API returned an error.")]
    DeepgramApiError {
        /// Error message from the Deepgram API.
        body: String,

        /// Underlying [`reqwest::Error`] from the HTTP request.
        err: reqwest::Error,
    },

    /// Something went wrong when generating the http request.
    #[error("Something went wrong when generating the http request: {0}")]
    HttpError(#[from] http::Error),

    /// Something went wrong when making the HTTP request.
    #[error("Something went wrong when making the HTTP request: {0}")]
    ReqwestError(#[from] reqwest::Error),

    /// Something went wrong during I/O.
    #[error("Something went wrong during I/O: {0}")]
    IoError(#[from] io::Error),

    #[cfg(feature = "live")]
    /// Something went wrong with WS.
    #[error("Something went wrong with WS: {0}")]
    WsError(#[from] tungstenite::Error),

    /// Something went wrong during serialization/deserialization.
    #[error("Something went wrong during serialization/deserialization: {0}")]
    SerdeError(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, DeepgramError>;

impl Deepgram {
    /// Construct a new Deepgram client.
    ///
    /// The client will be pointed at the deepgram API.
    ///
    /// Create your first API key on the [Deepgram Console][console].
    ///
    /// [console]: https://console.deepgram.com/
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`reqwest::Client::new`].
    pub fn new<K: AsRef<str>>(api_key: K) -> Self {
        Self::with_base_url(api_key, DEEPGRAM_BASE_URL)
    }

    /// Construct a new Deepgram client with the specified base URL.
    ///
    /// When using a self-hosted instance of deepgram, this will be the
    /// host portion of your own instance. For instance, if you would
    /// query your deepgram instance at `http://deepgram.internal/v1/listen`,
    /// the base_url will be `http://deepgram.internal`.
    ///
    /// Admin features, such as billing, usage, and key management will
    /// still go through the hosted site at `https://api.deepgram.com`.
    ///
    /// Create your first API key on the [Deepgram Console][console].
    ///
    /// [console]: https://console.deepgram.com/
    ///
    /// # Example:
    ///
    /// ```
    /// # use deepgram::Deepgram;
    /// let deepgram = Deepgram::with_base_url(
    ///     "apikey12345",
    ///     "http://localhost:8080",
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`reqwest::Client::new`], or if `base_url`
    /// is not a valid URL.
    ///
    ///

    pub fn with_base_url<K, U>(api_key: K, base_url: U) -> Self
    where
        K: AsRef<str>,
        U: TryInto<Url>,
        U::Error: std::fmt::Debug,
    {
        static USER_AGENT: &str = concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION"),
            " rust",
        );

        let authorization_header = {
            let mut header = HeaderMap::new();
            header.insert(
                "Authorization",
                HeaderValue::from_str(&format!("Token {}", api_key.as_ref()))
                    .expect("Invalid API key"),
            );
            header
        };
        let api_key = api_key.as_ref().to_owned();
        let base_url = base_url.try_into().expect("base_url must be a valid Url");
        Deepgram {
            api_key,
            base_url,
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .default_headers(authorization_header)
                .build()
                // Even though `reqwest::Client::new` is not used here, it will always panic under the same conditions
                .expect("See reqwest::Client::new docs for cause of panic"),
        }
    }
}

/// Sends the request and checks the response for an error.
///
/// If there is an error, it translates it into a [`DeepgramError::DeepgramApiError`].
/// Otherwise, it deserializes the JSON accordingly.
async fn send_and_translate_response<R: DeserializeOwned>(
    request_builder: RequestBuilder,
) -> crate::Result<R> {
    let response = request_builder.send().await?;

    match response.error_for_status_ref() {
        Ok(_) => Ok(response.json().await?),
        Err(err) => Err(DeepgramError::DeepgramApiError {
            body: response.text().await?,
            err,
        }),
    }
}
