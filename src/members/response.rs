//! Deepgram members API response types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::response::Message;

/// Returned by [`Members::list_members`](super::Members::list_members).
///
/// See the [Deepgram API Reference][api] for more info.
///
/// [api]: https://developers.deepgram.com/api-reference/#members-get-members
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Members {
    #[allow(missing_docs)]
    pub members: Vec<Member>,
}

/// Returned by [`Members::list_members`](super::Members::list_members).
///
/// See the [Deepgram API Reference][api] for more info.
///
/// [api]: https://developers.deepgram.com/api-reference/#members-get-members
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Member {
    #[allow(missing_docs)]
    pub member_id: Uuid,

    #[allow(missing_docs)]
    pub first_name: Option<String>,

    #[allow(missing_docs)]
    pub last_name: Option<String>,

    #[allow(missing_docs)]
    pub scopes: Vec<String>,

    #[allow(missing_docs)]
    pub email: String,
}
