use super::{Deepgram, Result};
use reqwest::RequestBuilder;

mod audio_source;
mod options;
mod response;

pub use audio_source::{BufferSource, UrlSource};
pub use options::{Language, Model, Options, OptionsBuilder, Redact, Utterances};
pub use response::PrerecordedResponse;

use audio_source::AudioSource;
use options::SerializableOptions;

impl<K: AsRef<str>> Deepgram<K> {
    pub async fn prerecorded_request(
        &self,
        source: impl AudioSource,
        options: &Options<'_>,
    ) -> Result<PrerecordedResponse> {
        let request_builder = self.make_prerecorded_request_builder(source, options);

        Ok(request_builder.send().await?.json().await?)
    }

    /// Makes a `reqwest::RequestBuilder` without actually sending the request.
    /// This allows you to modify the request before it is sent.
    ///
    /// Avoid using `make_prerecorded_request_builder` where possible.
    /// By customizing the request, there is less of a guarentee that it will conform to the Deepgram API.
    /// Prefer using `Deepgram::prerecorded_request`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deepgram::{
    /// #     prerecorded::{Language, Options, PrerecordedResponse, UrlSource},
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
    /// let response: PrerecordedResponse = customized_request_builder.send().await?.json().await?;
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
            .post("https://api.deepgram.com/v1/listen")
            .header("Authorization", format!("Token {}", self.api_key.as_ref()))
            .query(&SerializableOptions(options));
        let request_builder = source.fill_body(request_builder);

        request_builder
    }
}