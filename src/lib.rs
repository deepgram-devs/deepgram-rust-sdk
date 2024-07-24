#![forbid(unsafe_code)]
#![warn(missing_debug_implementations, missing_docs, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::derive_partial_eq_without_eq)]

//! Official Rust SDK for Deepgram's automated speech recognition APIs.
//!
//! Get started transcribing with a [`Transcription`] object.

use core::fmt;
use std::io;
use std::ops::Deref;

use reqwest::{
    header::{HeaderMap, HeaderValue},
    RequestBuilder,
};
use serde::de::DeserializeOwned;
use thiserror::Error;
use url::Url;

#[cfg(feature = "listen")]
pub mod common;
#[cfg(feature = "listen")]
pub mod listen;
#[cfg(feature = "manage")]
pub mod manage;
#[cfg(feature = "speak")]
pub mod speak;

static DEEPGRAM_BASE_URL: &str = "https://api.deepgram.com";

/// Transcribe audio using Deepgram's automated speech recognition.
///
/// Constructed using [`Deepgram::transcription`].
///
/// See the [Deepgram API Reference][api] for more info.
///
/// [api]: https://developers.deepgram.com/api-reference/#transcription
#[derive(Debug, Clone)]
pub struct Transcription<'a>(#[allow(unused)] pub &'a Deepgram);

/// Generate speech from text using Deepgram's text to speech api.
///
/// Constructed using [`Deepgram::text_to_speech`].
///
/// See the [Deepgram API Reference][api] for more info.
///
/// [api]: https://developers.deepgram.com/reference/text-to-speech-api
#[derive(Debug, Clone)]
pub struct Speak<'a>(#[allow(unused)] pub &'a Deepgram);

impl Deepgram {
    /// Construct a new [`Transcription`] from a [`Deepgram`].
    pub fn transcription(&self) -> Transcription<'_> {
        self.into()
    }

    /// Construct a new [`Speak`] from a [`Deepgram`].
    pub fn text_to_speech(&self) -> Speak<'_> {
        self.into()
    }
}

impl<'a> From<&'a Deepgram> for Transcription<'a> {
    /// Construct a new [`Transcription`] from a [`Deepgram`].
    fn from(deepgram: &'a Deepgram) -> Self {
        Self(deepgram)
    }
}

impl<'a> From<&'a Deepgram> for Speak<'a> {
    /// Construct a new [`Speak`] from a [`Deepgram`].
    fn from(deepgram: &'a Deepgram) -> Self {
        Self(deepgram)
    }
}

impl<'a> Transcription<'a> {
    /// Expose a method to access the inner `Deepgram` reference if needed.
    pub fn deepgram(&self) -> &Deepgram {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RedactedString(pub String);

impl fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl Deref for RedactedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A client for the Deepgram API.
///
/// Make transcriptions requests using [`Deepgram::transcription`].
#[derive(Debug, Clone)]
pub struct Deepgram {
    #[cfg_attr(not(feature = "listen"), allow(unused))]
    api_key: Option<RedactedString>,
    #[cfg_attr(not(feature = "listen"), allow(unused))]
    base_url: Url,
    #[cfg_attr(not(feature = "listen"), allow(unused))]
    client: reqwest::Client,
}

/// Errors that may arise from the [`deepgram`](crate) crate.
// TODO sub-errors for the different types?
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DeepgramError {
    /// The provided URL could not be used as a base_url.
    #[error("The provided base_url is not valid. Please provide a URL starting with http:// or https://")]
    InvalidBaseUrl,

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

    #[cfg(feature = "listen")]
    /// Something went wrong with WS.
    #[error("Something went wrong with WS: {0}")]
    WsError(#[from] tungstenite::Error),

    /// Something went wrong during serialization/deserialization.
    #[error("Something went wrong during serialization/deserialization: {0}")]
    SerdeError(#[from] serde_json::Error),
}

#[cfg_attr(not(feature = "listen"), allow(unused))]
type Result<T> = std::result::Result<T, DeepgramError>;

impl Deepgram {
    /// Construct a new Deepgram client.
    ///
    /// The client will be pointed at Deepgram's hosted API.
    ///
    /// Create your first API key on the [Deepgram Console][console].
    ///
    /// [console]: https://console.deepgram.com/
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`reqwest::Client::new`].
    pub fn new<K: AsRef<str>>(api_key: K) -> Self {
        let api_key = Some(api_key.as_ref().to_owned());
        // Unwrap: `DEEPGRAM_BASE_URL` is a valid URL.
        let base_url = parse_base_url(DEEPGRAM_BASE_URL).unwrap();
        Self::inner_constructor(base_url, api_key)
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
    /// Self-hosted instances do not in general authenticate incoming
    /// requests, so unlike in [`Deepgram::new`], so no api key needs to be
    /// provided. The SDK will not include an `Authorization` header in its
    /// requests. If an API key is required, consider using
    /// [`Deepgram::with_base_url_and_api_key`].
    ///
    /// [console]: https://console.deepgram.com/
    ///
    /// # Example:
    ///
    /// ```
    /// # use deepgram::Deepgram;
    /// let deepgram = Deepgram::with_base_url(
    ///     "http://localhost:8080",
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`DeepgramError::InvalidBaseUrl`] if base_url is not a valid URL.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`reqwest::Client::new`].
    pub fn with_base_url<U>(base_url: U) -> Result<Self>
    where
        U: TryInto<Url>,
    {
        let base_url = parse_base_url(base_url)?;
        Ok(Self::inner_constructor(base_url, None))
    }

    /// Construct a new Deepgram client with the specified base URL and
    /// API Key.
    ///
    /// When using a self-hosted instance of deepgram, this will be the
    /// host portion of your own instance. For instance, if you would
    /// query your deepgram instance at `http://deepgram.internal/v1/listen`,
    /// the base_url will be `http://deepgram.internal`.
    ///
    /// Admin features, such as billing, usage, and key management will
    /// still go through the hosted site at `https://api.deepgram.com`.
    ///
    /// [console]: https://console.deepgram.com/
    ///
    /// # Example:
    ///
    /// ```
    /// # use deepgram::Deepgram;
    /// let deepgram = Deepgram::with_base_url_and_api_key(
    ///     "http://localhost:8080",
    ///     "apikey12345",
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`DeepgramError::InvalidBaseUrl`] if base_url is not a valid URL.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`reqwest::Client::new`].
    pub fn with_base_url_and_api_key<U, K>(base_url: U, api_key: K) -> Result<Self>
    where
        U: TryInto<Url>,
        K: AsRef<str>,
    {
        let base_url = parse_base_url(base_url)?;
        Ok(Self::inner_constructor(
            base_url,
            Some(api_key.as_ref().to_owned()),
        ))
    }

    fn inner_constructor(base_url: Url, api_key: Option<String>) -> Self {
        static USER_AGENT: &str = concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION"),
            " rust",
        );

        let authorization_header = {
            let mut header = HeaderMap::new();
            if let Some(api_key) = &api_key {
                header.insert(
                    "Authorization",
                    HeaderValue::from_str(&format!("Token {}", api_key)).expect("Invalid API key"),
                );
            }
            header
        };

        Deepgram {
            api_key: api_key.map(RedactedString),
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

fn parse_base_url<U>(base_url: U) -> Result<Url>
where
    U: TryInto<Url>,
{
    let Ok(base_url) = base_url.try_into() else {
        return Err(DeepgramError::InvalidBaseUrl);
    };
    if base_url.cannot_be_a_base() {
        return Err(DeepgramError::InvalidBaseUrl);
    }

    if !matches!(base_url.scheme(), "http" | "https" | "ws" | "wss") {
        return Err(DeepgramError::InvalidBaseUrl);
    }

    Ok(base_url)
}

/// Sends the request and checks the response for an error.
///
/// If there is an error, it translates it into a [`DeepgramError::DeepgramApiError`].
/// Otherwise, it deserializes the JSON accordingly.
#[cfg_attr(not(feature = "listen"), allow(unused))]
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
