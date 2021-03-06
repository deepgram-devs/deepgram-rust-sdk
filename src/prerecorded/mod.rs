#![warn(missing_docs)]

//! This module provides various types that are used for pre-recorded requests to Deepgram.

use crate::{Deepgram, DeepgramError};
use reqwest::RequestBuilder;

mod audio_source;
mod options;
mod response;

pub use audio_source::{BufferSource, UrlSource};
pub use options::{Keyword, Language, Model, Options, OptionsBuilder, Redact};
pub use response::{
    CallbackResponse, ChannelResult, Hit, ListenMetadata, ListenResults, Response,
    ResultAlternative, SearchResults, Utterance, Word,
};

use audio_source::AudioSource;
use options::SerializableOptions;
use serde::de::DeserializeOwned;

static DEEPGRAM_API_URL_LISTEN: &str = "https://api.deepgram.com/v1/listen";

impl<K: AsRef<str>> Deepgram<K> {
    /// Sends a request to Deepgram to transcribe pre-recorded audio.
    /// If you wish to use the Callback feature, you should use [`Deepgram::callback_request`] instead.
    ///
    /// The `source` parameter is either a [`BufferSource`] or a [`UrlSource`].
    ///
    /// See the [Deepgram API Reference][api] for more info.
    ///
    /// [api]: https://developers.deepgram.com/api-reference/#transcription-prerecorded
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deepgram::{
    /// #     prerecorded::{Language, Options, UrlSource},
    /// #     Deepgram, DeepgramError,
    /// # };
    /// # use std::env;
    /// #
    /// # static AUDIO_URL: &str = "https://static.deepgram.com/examples/Bueller-Life-moves-pretty-fast.wav";
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), DeepgramError> {
    /// # let deepgram_api_key =
    /// #     env::var("DEEPGRAM_API_KEY").expect("DEEPGRAM_API_KEY environmental variable");
    /// #
    /// let dg_client = Deepgram::new(&deepgram_api_key);
    ///
    /// let source = UrlSource { url: AUDIO_URL };
    ///
    /// let options = Options::builder()
    ///     .punctuate(true)
    ///     .language(Language::en_US)
    ///     .build();
    ///
    /// let response = dg_client.prerecorded_request(&source, &options).await?;
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub async fn prerecorded_request(
        &self,
        source: impl AudioSource,
        options: &Options<'_>,
    ) -> crate::Result<Response> {
        let request_builder = self.make_prerecorded_request_builder(source, options);

        send_and_translate_response(request_builder).await
    }

    /// Sends a request to Deepgram to transcribe pre-recorded audio using the Callback feature.
    /// Otherwise behaves similarly to [`Deepgram::prerecorded_request`].
    ///
    /// The `source` parameter is either a [`BufferSource`] or a [`UrlSource`].
    ///
    /// See the [Deepgram Callback feature docs][docs] for more info.
    ///
    /// [docs]: https://developers.deepgram.com/documentation/features/callback/
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deepgram::{
    /// #     prerecorded::{Language, Options, UrlSource},
    /// #     Deepgram, DeepgramError,
    /// # };
    /// # use std::env;
    /// #
    /// # static AUDIO_URL: &str = "https://static.deepgram.com/examples/Bueller-Life-moves-pretty-fast.wav";
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), DeepgramError> {
    /// # let deepgram_api_key =
    /// #     env::var("DEEPGRAM_API_KEY").expect("DEEPGRAM_API_KEY environmental variable");
    /// #
    /// let dg_client = Deepgram::new(&deepgram_api_key);
    ///
    /// let source = UrlSource { url: AUDIO_URL };
    ///
    /// let options = Options::builder()
    ///     .punctuate(true)
    ///     .language(Language::en_US)
    ///     .build();
    ///
    /// # let callback_url =
    /// #     env::var("DEEPGRAM_CALLBACK_URL").expect("DEEPGRAM_CALLBACK_URL environmental variable");
    /// #
    /// let response = dg_client
    ///     .callback_request(&source, &options, &callback_url)
    ///     .await?;
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub async fn callback_request(
        &self,
        source: impl AudioSource,
        options: &Options<'_>,
        callback: &str,
    ) -> crate::Result<CallbackResponse> {
        let request_builder = self.make_callback_request_builder(source, options, callback);

        send_and_translate_response(request_builder).await
    }

    /// Makes a [`reqwest::RequestBuilder`] without actually sending the request.
    /// This allows you to modify the request before it is sent.
    ///
    /// Avoid using this where possible.
    /// By customizing the request, there is less of a guarentee that it will conform to the Deepgram API.
    /// Prefer using [`Deepgram::prerecorded_request`].
    ///
    /// The `source` parameter is either a [`BufferSource`] or a [`UrlSource`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deepgram::{
    /// #     prerecorded::{Language, Options, Response, UrlSource},
    /// #     Deepgram,
    /// # };
    /// # use std::env;
    /// #
    /// # static AUDIO_URL: &str = "https://static.deepgram.com/examples/Bueller-Life-moves-pretty-fast.wav";
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> reqwest::Result<()> {
    /// #
    /// # let deepgram_api_key =
    /// #     env::var("DEEPGRAM_API_KEY").expect("DEEPGRAM_API_KEY environmental variable");
    /// #
    /// # let dg_client = Deepgram::new(&deepgram_api_key);
    /// #
    /// # let source = UrlSource { url: AUDIO_URL };
    /// #
    /// # let options = Options::builder()
    /// #     .punctuate(true)
    /// #     .language(Language::en_US)
    /// #     .build();
    /// #
    /// let request_builder = dg_client.make_prerecorded_request_builder(&source, &options);
    ///
    /// // Customize the RequestBuilder here
    /// let customized_request_builder = request_builder
    ///     .query(&[("custom_query_key", "custom_query_value")])
    ///     .header("custom_header_key", "custom_header_value");
    ///
    /// // It is necessary to annotate the type of response here
    /// // That way it knows what type to deserialize the JSON into
    /// let response: Response = customized_request_builder.send().await?.json().await?;
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub fn make_prerecorded_request_builder(
        &self,
        source: impl AudioSource,
        options: &Options<'_>,
    ) -> RequestBuilder {
        let request_builder = self
            .client
            .post(DEEPGRAM_API_URL_LISTEN)
            .header("Authorization", format!("Token {}", self.api_key.as_ref()))
            .query(&SerializableOptions(options));

        source.fill_body(request_builder)
    }

    /// Similar to [`Deepgram::make_prerecorded_request_builder`],
    /// but for the purposes of a [callback request][callback].
    ///
    /// You should avoid using this where possible too, preferring [`Deepgram::callback_request`].
    ///
    /// The `source` parameter is either a [`BufferSource`] or a [`UrlSource`].
    ///
    /// [callback]: https://developers.deepgram.com/documentation/features/callback/
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deepgram::{
    /// #     prerecorded::{CallbackResponse, Language, Options, UrlSource},
    /// #     Deepgram,
    /// # };
    /// # use std::env;
    /// #
    /// # static AUDIO_URL: &str = "https://static.deepgram.com/examples/Bueller-Life-moves-pretty-fast.wav";
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> reqwest::Result<()> {
    /// #
    /// # let deepgram_api_key =
    /// #     env::var("DEEPGRAM_API_KEY").expect("DEEPGRAM_API_KEY environmental variable");
    /// #
    /// # let dg_client = Deepgram::new(&deepgram_api_key);
    /// #
    /// # let source = UrlSource { url: AUDIO_URL };
    /// #
    /// # let options = Options::builder()
    /// #     .punctuate(true)
    /// #     .language(Language::en_US)
    /// #     .build();
    /// #
    /// # let callback_url =
    /// #     env::var("DEEPGRAM_CALLBACK_URL").expect("DEEPGRAM_CALLBACK_URL environmental variable");
    /// #
    /// let request_builder = dg_client.make_callback_request_builder(&source, &options, &callback_url);
    ///
    /// // Customize the RequestBuilder here
    /// let customized_request_builder = request_builder
    ///     .query(&[("custom_query_key", "custom_query_value")])
    ///     .header("custom_header_key", "custom_header_value");
    ///
    /// // It is necessary to annotate the type of response here
    /// // That way it knows what type to deserialize the JSON into
    /// let response: CallbackResponse = customized_request_builder.send().await?.json().await?;
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub fn make_callback_request_builder(
        &self,
        source: impl AudioSource,
        options: &Options<'_>,
        callback: &str,
    ) -> RequestBuilder {
        self.make_prerecorded_request_builder(source, options)
            .query(&[("callback", callback)])
    }
}

async fn send_and_translate_response<R: DeserializeOwned>(
    request_builder: RequestBuilder,
) -> crate::Result<R> {
    let response = request_builder.send().await?;

    match response.error_for_status_ref() {
        Ok(_) => Ok(response.json().await?),
        Err(err) => Err(DeepgramError::TranscriptionError {
            body: response.text().await?,
            err,
        }),
    }
}
