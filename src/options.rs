use std::fmt;
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

/// Controls common to every terminal operation.
///
/// Drop the returned future to cancel a request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RequestOptions {
    /// Retry attempts after the first request. Overrides the client default.
    pub max_retries: Option<u32>,
    /// Timeout for each attempt. Overrides the client default.
    pub timeout: Option<Duration>,
}

impl RequestOptions {
    /// Creates empty request controls that use the client defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the retry attempts after the first request.
    pub fn max_retries(mut self, value: u32) -> Self {
        self.max_retries = Some(value);
        self
    }

    /// Sets the timeout for each attempt.
    pub fn timeout(mut self, value: Duration) -> Self {
        self.timeout = Some(value);
        self
    }
}

/// The maximum accepted age of returned data.
///
/// Older answers cost less. The API default is 24 hours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Freshness {
    /// Read the platform now.
    #[serde(rename = "live")]
    Live,
    /// Accept data up to 24 hours old.
    #[serde(rename = "24h")]
    TwentyFourHours,
    /// Accept data up to 7 days old.
    #[serde(rename = "7d")]
    SevenDays,
    /// Accept data up to 30 days old.
    #[serde(rename = "30d")]
    ThirtyDays,
}

impl Freshness {
    /// Returns the wire value, such as `24h`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::TwentyFourHours => "24h",
            Self::SevenDays => "7d",
            Self::ThirtyDays => "30d",
        }
    }
}

impl fmt::Display for Freshness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A supported social platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Platform {
    /// Instagram.
    Instagram,
    /// TikTok.
    #[serde(rename = "tiktok")]
    TikTok,
    /// X, formerly Twitter.
    Twitter,
    /// Reddit.
    Reddit,
    /// A platform this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

impl Platform {
    /// Returns the wire value, such as `instagram`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instagram => "instagram",
            Self::TikTok => "tiktok",
            Self::Twitter => "twitter",
            Self::Reddit => "reddit",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Where the answer came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Source {
    /// The platform answered just now.
    Live,
    /// The answer came from the cache.
    Cache,
    /// A source this SDK version does not know yet.
    #[serde(other)]
    Unknown,
}

/// Options for [`Client::fetch`](crate::Client::fetch).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FetchOptions {
    /// Maximum accepted data age. The API default is 24 hours.
    pub freshness: Option<Freshness>,
    /// Retry and timeout controls for this request.
    pub request_options: RequestOptions,
}

impl FetchOptions {
    /// Creates options that use the API and client defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum accepted data age.
    pub fn freshness(mut self, value: Freshness) -> Self {
        self.freshness = Some(value);
        self
    }

    /// Sets the retry attempts after the first request.
    pub fn max_retries(mut self, value: u32) -> Self {
        self.request_options.max_retries = Some(value);
        self
    }

    /// Sets the timeout for each attempt.
    pub fn timeout(mut self, value: Duration) -> Self {
        self.request_options.timeout = Some(value);
        self
    }
}

/// A value that can be sent as an operation query parameter.
pub(crate) trait QueryValue {
    fn encode(&self) -> Option<String>;
}

impl QueryValue for String {
    fn encode(&self) -> Option<String> {
        (!self.is_empty()).then(|| self.clone())
    }
}

impl QueryValue for i64 {
    fn encode(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl QueryValue for bool {
    fn encode(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl QueryValue for Freshness {
    fn encode(&self) -> Option<String> {
        Some(self.as_str().to_owned())
    }
}

impl QueryValue for Platform {
    fn encode(&self) -> Option<String> {
        Some(self.as_str().to_owned())
    }
}

impl QueryValue for DateTime<Utc> {
    fn encode(&self) -> Option<String> {
        Some(self.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

impl<T: QueryValue> QueryValue for Option<T> {
    fn encode(&self) -> Option<String> {
        self.as_ref().and_then(QueryValue::encode)
    }
}

/// Encoded query parameters of one operation request.
#[derive(Clone, Debug, Default)]
pub(crate) struct Query(pub(crate) Vec<(&'static str, String)>);

impl Query {
    pub(crate) fn push(&mut self, name: &'static str, value: &impl QueryValue) {
        if let Some(encoded) = value.encode() {
            self.0.push((name, encoded));
        }
    }

    pub(crate) fn require(
        &mut self,
        name: &'static str,
        value: &impl QueryValue,
    ) -> Result<(), crate::Error> {
        match value.encode() {
            Some(encoded) => {
                self.0.push((name, encoded));
                Ok(())
            }
            None => Err(crate::Error::invalid_argument(format!(
                "option {name} is required"
            ))),
        }
    }

    pub(crate) fn set_cursor(&mut self, cursor: &str) {
        self.0.retain(|(name, _)| *name != "cursor");
        self.0.push(("cursor", cursor.to_owned()));
    }
}
