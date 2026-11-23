//! Verification of the webhooks that Openhandle pushes to your endpoints.
//!
//! ```no_run
//! use openhandle::webhooks::{self, VerifyOptions};
//!
//! # fn handle(body: &[u8], headers: &reqwest::header::HeaderMap) -> Result<(), webhooks::WebhookError> {
//! let secret = std::env::var("OPENHANDLE_WEBHOOK_SECRET").unwrap_or_default();
//! let event = webhooks::verify(body, headers, &secret, VerifyOptions::new())?;
//! println!("{} {}", event.r#type, event.id);
//! # Ok(())
//! # }
//! ```

use std::error::Error as StdError;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use reqwest::header::HeaderMap;
use sha2::Sha256;

use crate::generated::models::WebhookEvent;

/// The header that holds the `sha256=` signature.
pub const SIGNATURE_HEADER: &str = "x-openhandle-signature";
/// The header that holds the Unix timestamp in seconds.
pub const TIMESTAMP_HEADER: &str = "x-openhandle-timestamp";
/// The default maximum difference between the timestamp and the current time.
pub const DEFAULT_TOLERANCE: Duration = Duration::from_secs(300);

const SIGNATURE_PREFIX: &str = "sha256=";

/// Controls for [`verify`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VerifyOptions {
    /// Maximum difference between the timestamp and `now`, in both directions.
    pub tolerance: Duration,
    /// The time to verify against. `None` uses the current time. Set it in tests.
    pub now: Option<SystemTime>,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            tolerance: DEFAULT_TOLERANCE,
            now: None,
        }
    }
}

impl VerifyOptions {
    /// Creates options with a 300 second tolerance and the current time.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum difference between the timestamp and `now`.
    pub fn tolerance(mut self, value: Duration) -> Self {
        self.tolerance = value;
        self
    }

    /// Sets the time to verify against.
    pub fn now(mut self, value: SystemTime) -> Self {
        self.now = Some(value);
        self
    }
}

/// The reason a webhook failed verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WebhookErrorCode {
    /// The `X-Openhandle-Signature` header is missing.
    MissingSignature,
    /// The `X-Openhandle-Timestamp` header is missing.
    MissingTimestamp,
    /// The timestamp is not a base-10 integer of Unix seconds.
    InvalidTimestamp,
    /// The timestamp is further from `now` than the tolerance.
    TimestampOutOfTolerance,
    /// The signature does not match the body, the timestamp, and the secret.
    InvalidSignature,
    /// The signed body is not a JSON webhook event object.
    InvalidPayload,
}

impl WebhookErrorCode {
    /// Returns the stable code, such as `invalid_signature`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingSignature => "missing_signature",
            Self::MissingTimestamp => "missing_timestamp",
            Self::InvalidTimestamp => "invalid_timestamp",
            Self::TimestampOutOfTolerance => "timestamp_out_of_tolerance",
            Self::InvalidSignature => "invalid_signature",
            Self::InvalidPayload => "invalid_payload",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::MissingSignature => "the X-Openhandle-Signature header is missing",
            Self::MissingTimestamp => "the X-Openhandle-Timestamp header is missing",
            Self::InvalidTimestamp => "the X-Openhandle-Timestamp header is not Unix seconds",
            Self::TimestampOutOfTolerance => "the webhook timestamp is outside the tolerance",
            Self::InvalidSignature => "the webhook signature does not match",
            Self::InvalidPayload => "the webhook body is not a JSON event object",
        }
    }
}

impl fmt::Display for WebhookErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A webhook that failed verification.
///
/// Branch on [`WebhookError::code`], never on the message.
#[derive(Debug)]
pub struct WebhookError {
    code: WebhookErrorCode,
    cause: Option<serde_json::Error>,
}

impl WebhookError {
    fn new(code: WebhookErrorCode) -> Self {
        Self { code, cause: None }
    }

    /// Returns the reason verification failed.
    pub fn code(&self) -> WebhookErrorCode {
        self.code
    }
}

impl fmt::Display for WebhookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "openhandle: {}: {}",
            self.code.as_str(),
            self.code.message()
        )
    }
}

impl StdError for WebhookError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause as &(dyn StdError + 'static))
    }
}

/// Verifies a pushed webhook and returns its event.
///
/// Pass the raw request body exactly as received. Do not parse and serialize
/// it first. The function sends no request and needs no API key.
pub fn verify(
    body: &[u8],
    headers: &HeaderMap,
    secret: &str,
    options: VerifyOptions,
) -> Result<WebhookEvent, WebhookError> {
    let signature = headers
        .get(SIGNATURE_HEADER)
        .filter(|value| !value.is_empty())
        .ok_or(WebhookError::new(WebhookErrorCode::MissingSignature))?;
    let timestamp = headers
        .get(TIMESTAMP_HEADER)
        .filter(|value| !value.is_empty())
        .ok_or(WebhookError::new(WebhookErrorCode::MissingTimestamp))?
        .as_bytes();

    let seconds =
        parse_timestamp(timestamp).ok_or(WebhookError::new(WebhookErrorCode::InvalidTimestamp))?;
    let now = unix_seconds(options.now.unwrap_or_else(SystemTime::now));
    if (now - seconds).unsigned_abs() > u128::from(options.tolerance.as_secs()) {
        return Err(WebhookError::new(WebhookErrorCode::TimestampOutOfTolerance));
    }

    let received = signature
        .as_bytes()
        .strip_prefix(SIGNATURE_PREFIX.as_bytes())
        .and_then(decode_lower_hex)
        .ok_or(WebhookError::new(WebhookErrorCode::InvalidSignature))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts a key of any length");
    mac.update(timestamp);
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&received)
        .map_err(|_| WebhookError::new(WebhookErrorCode::InvalidSignature))?;

    let invalid_payload = |cause| WebhookError {
        code: WebhookErrorCode::InvalidPayload,
        cause: Some(cause),
    };
    let object: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(body).map_err(invalid_payload)?;
    let named = ["id", "type", "environment", "createdAt"]
        .iter()
        .all(|field| object.get(*field).is_some_and(serde_json::Value::is_string));
    if !named || !object.get("data").is_some_and(serde_json::Value::is_object) {
        return Err(WebhookError::new(WebhookErrorCode::InvalidPayload));
    }
    serde_json::from_value(serde_json::Value::Object(object)).map_err(invalid_payload)
}

fn parse_timestamp(value: &[u8]) -> Option<i128> {
    if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(value)
        .ok()?
        .parse::<i64>()
        .ok()
        .map(i128::from)
}

fn unix_seconds(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i128::from(elapsed.as_secs()),
        Err(error) => -i128::from(error.duration().as_secs()),
    }
}

fn decode_lower_hex(value: &[u8]) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    let nibble = |digit: u8| match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    };
    value
        .chunks_exact(2)
        .map(|pair| Some(nibble(pair[0])? << 4 | nibble(pair[1])?))
        .collect()
}

#[cfg(test)]
mod conformance;
