use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url as ParsedUrl;

use crate::errors::Error;

pub(crate) const DEFAULT_BASE_URL: &str = "https://api.openhandle.dev";
pub(crate) const DEFAULT_MAX_RETRIES: u32 = 2;
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const CLIENT_HEADER: &str = concat!("openhandle-rust/", env!("CARGO_PKG_VERSION"));

/// A reusable Openhandle API client.
///
/// The API key selects the Test or Live environment. There is no separate
/// environment option. Cloning a client is cheap and shares its connection
/// pool.
#[derive(Clone)]
pub struct Client {
    pub(crate) core: Arc<ClientCore>,
}

pub(crate) struct ClientCore {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) http_client: reqwest::Client,
    pub(crate) max_retries: u32,
    pub(crate) timeout: Duration,
}

impl Client {
    /// Creates a client with the default configuration.
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder(api_key).build()
    }

    /// Starts a client configuration.
    pub fn builder(api_key: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            http_client: None,
            max_retries: DEFAULT_MAX_RETRIES,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("base_url", &self.core.base_url)
            .field("max_retries", &self.core.max_retries)
            .field("timeout", &self.core.timeout)
            .finish_non_exhaustive()
    }
}

/// Configures a [`Client`].
#[must_use]
pub struct ClientBuilder {
    api_key: String,
    base_url: String,
    http_client: Option<reqwest::Client>,
    max_retries: u32,
    timeout: Duration,
}

impl ClientBuilder {
    /// Overrides the API origin, mainly for proxies and tests.
    pub fn base_url(mut self, value: impl Into<String>) -> Self {
        self.base_url = value.into().trim().trim_end_matches('/').to_owned();
        self
    }

    /// Supplies the HTTP client used for requests.
    pub fn http_client(mut self, value: reqwest::Client) -> Self {
        self.http_client = Some(value);
        self
    }

    /// Sets retry attempts after the first request. The default is 2.
    pub fn max_retries(mut self, value: u32) -> Self {
        self.max_retries = value;
        self
    }

    /// Sets the default timeout for each attempt. The default is 30 seconds.
    pub fn timeout(mut self, value: Duration) -> Self {
        self.timeout = value;
        self
    }

    /// Validates the configuration and creates the client.
    pub fn build(self) -> Result<Client, Error> {
        let api_key = self.api_key.trim().to_owned();
        if api_key.is_empty() {
            return Err(Error::invalid_argument("API key must not be empty"));
        }
        if self.timeout.is_zero() {
            return Err(Error::invalid_argument("timeout must be positive"));
        }
        let parsed = ParsedUrl::parse(&self.base_url)
            .ok()
            .filter(|parsed| parsed.has_host());
        let Some(parsed) = parsed else {
            return Err(Error::invalid_argument(
                "base URL must be an absolute HTTP or HTTPS URL",
            ));
        };
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(Error::invalid_argument("base URL must use HTTP or HTTPS"));
        }
        let http_client = match self.http_client {
            Some(http_client) => http_client,
            None => reqwest::Client::builder().build().map_err(|error| {
                Error::invalid_argument(format!("HTTP client could not be created: {error}"))
            })?,
        };
        Ok(Client {
            core: Arc::new(ClientCore {
                api_key,
                base_url: parsed.as_str().trim_end_matches('/').to_owned(),
                http_client,
                max_retries: self.max_retries,
                timeout: self.timeout,
            }),
        })
    }
}

impl fmt::Debug for ClientBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClientBuilder")
            .field("base_url", &self.base_url)
            .field("max_retries", &self.max_retries)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}
