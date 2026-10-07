use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// Shared, cloneable source of an error.
pub type ErrorCause = Arc<dyn StdError + Send + Sync + 'static>;

/// Every failure returned by the SDK.
///
/// Branch on [`ApiError::code`], never on a message.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The Openhandle API or the HTTP transport failed.
    Api(Box<ApiError>),
    /// A reference was invalid. No request was made.
    Reference(ReferenceError),
    /// A social URL belongs to a different platform or resource than the
    /// selector. No request was made.
    ReferenceMismatch(ReferenceMismatchError),
    /// The client configuration or the operation options were invalid. No
    /// request was made.
    InvalidArgument(String),
}

impl Error {
    /// Returns the API or transport error, if this is one.
    pub fn as_api(&self) -> Option<&ApiError> {
        match self {
            Self::Api(error) => Some(error.as_ref()),
            _ => None,
        }
    }

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self::InvalidArgument(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(error) => error.fmt(formatter),
            Self::Reference(error) => error.fmt(formatter),
            Self::ReferenceMismatch(error) => error.fmt(formatter),
            Self::InvalidArgument(message) => write!(formatter, "openhandle: {message}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Api(error) => error.source(),
            Self::Reference(error) => error.source(),
            Self::ReferenceMismatch(_) | Self::InvalidArgument(_) => None,
        }
    }
}

impl From<ApiError> for Error {
    fn from(error: ApiError) -> Self {
        Self::Api(Box::new(error))
    }
}

impl From<ReferenceError> for Error {
    fn from(error: ReferenceError) -> Self {
        Self::Reference(error)
    }
}

impl From<ReferenceMismatchError> for Error {
    fn from(error: ReferenceMismatchError) -> Self {
        Self::ReferenceMismatch(error)
    }
}

/// An error returned by the Openhandle API or the transport runtime.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ApiError {
    /// Stable machine-readable code, such as `PROFILE_NOT_FOUND`.
    pub code: String,
    /// Human-readable explanation. Do not branch on it.
    pub message: String,
    /// Request identifier to include in logs and support requests.
    pub request_id: Option<String>,
    /// Whether the same request may succeed when sent again.
    pub retryable: bool,
    /// Delay requested by the `Retry-After` header.
    pub retry_after: Option<Duration>,
    /// HTTP status, absent for transport failures.
    pub status: Option<u16>,
    /// Structured details returned by the API.
    pub details: Option<serde_json::Map<String, serde_json::Value>>,
    /// Underlying transport or decoding failure.
    pub cause: Option<ErrorCause>,
}

impl ApiError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            request_id: None,
            retryable: false,
            retry_after: None,
            status: None,
            details: None,
            cause: None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.request_id {
            Some(request_id) => write!(
                formatter,
                "openhandle: {}: {} (request {request_id})",
                self.code, self.message
            ),
            None => write!(formatter, "openhandle: {}: {}", self.code, self.message),
        }
    }
}

impl StdError for ApiError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn StdError + 'static))
    }
}

/// A locally invalid resource reference. It is returned before any request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ReferenceError {
    /// Explanation of the invalid reference.
    pub message: String,
    /// Underlying parsing failure.
    pub cause: Option<ErrorCause>,
}

impl ReferenceError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cause: None,
        }
    }
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "openhandle: {}", self.message)
    }
}

impl StdError for ReferenceError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn StdError + 'static))
    }
}

/// A social URL for a different resource than the selector it was given to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferenceMismatchError {
    /// Platform of the selector.
    pub expected_platform: String,
    /// Resource of the selector.
    pub expected_resource: String,
    /// Platform the URL belongs to.
    pub actual_platform: String,
    /// Resource the URL represents.
    pub actual_resource: String,
}

impl fmt::Display for ReferenceMismatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "openhandle: expected a {} {} URL, received a {} {} URL",
            self.expected_platform,
            self.expected_resource,
            self.actual_platform,
            self.actual_resource
        )
    }
}

impl StdError for ReferenceMismatchError {}
