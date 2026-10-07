use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::DateTime;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, RETRY_AFTER};
use reqwest::{Method, StatusCode, Url as ParsedUrl};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::client::{CLIENT_HEADER, Client};
use crate::errors::{ApiError, Error, ErrorCause};
use crate::generated::resources::FetchResource;
use crate::options::{Platform, Query, RequestOptions};
use crate::references::{Reference, resolve_reference};
use crate::response::{Billing, Page, Response, ResponseWire};

const MAX_RESPONSE_BYTES: usize = 32 << 20;

/// The client, bound selector values, and first reference failure of one
/// node in the resource graph. Selecting never performs a request.
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    client: Client,
    bindings: Vec<(&'static str, String)>,
    reference_error: Option<Error>,
}

impl Scope {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            bindings: Vec::new(),
            reference_error: None,
        }
    }

    pub(crate) fn select(
        &self,
        parameter: &'static str,
        platform: &str,
        resource: &str,
        reference: Reference,
    ) -> Self {
        let mut scope = self.clone();
        match resolve_reference(&reference, platform, resource) {
            Ok(value) => scope.bindings.push((parameter, value)),
            Err(error) => {
                scope.reference_error.get_or_insert(error);
            }
        }
        scope
    }

    pub(crate) fn operation(
        &self,
        method: Method,
        api_path: &'static str,
        query: Result<Query, Error>,
        controls: RequestOptions,
    ) -> Result<OperationRequest, Error> {
        if let Some(error) = &self.reference_error {
            return Err(error.clone());
        }
        let query = query?;
        let mut path = String::with_capacity(api_path.len());
        let mut rest = api_path;
        while let Some(start) = rest.find('{') {
            let end = rest[start..]
                .find('}')
                .map(|offset| start + offset)
                .ok_or_else(|| {
                    Error::invalid_argument(format!("invalid operation path {api_path}"))
                })?;
            let name = &rest[start + 1..end];
            let value = self
                .bindings
                .iter()
                .rev()
                .find(|(parameter, _)| *parameter == name)
                .map(|(_, value)| value.as_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    Error::invalid_argument(format!("missing bound reference {name}"))
                })?;
            path.push_str(&rest[..start]);
            path.push_str(&encode_path_segment(value));
            rest = &rest[end + 1..];
        }
        path.push_str(rest);
        Ok(OperationRequest {
            client: self.client.clone(),
            method,
            path,
            query,
            body: None,
            controls,
        })
    }

    pub(crate) fn fetch(
        &self,
        url: &str,
        freshness: Option<crate::Freshness>,
        controls: RequestOptions,
    ) -> Result<OperationRequest, Error> {
        if url.trim().is_empty() {
            return Err(Error::invalid_argument("fetch URL must not be empty"));
        }
        let mut body = serde_json::Map::new();
        body.insert("url".to_owned(), url.into());
        if let Some(freshness) = freshness {
            body.insert("freshness".to_owned(), freshness.as_str().into());
        }
        Ok(OperationRequest {
            client: self.client.clone(),
            method: Method::POST,
            path: "/v1/urls/fetch".to_owned(),
            query: Query::default(),
            body: Some(serde_json::Value::Object(body).to_string()),
            controls,
        })
    }
}

/// One fully bound operation request. Pages keep it to request the next
/// cursor with the same operation and options.
#[derive(Clone)]
pub(crate) struct OperationRequest {
    client: Client,
    method: Method,
    path: String,
    query: Query,
    body: Option<String>,
    controls: RequestOptions,
}

struct RawResponse {
    body: Vec<u8>,
    request_id: Option<String>,
    billing: Billing,
}

impl OperationRequest {
    pub(crate) fn with_cursor(&self, cursor: &str) -> Self {
        let mut request = self.clone();
        request.query.set_cursor(cursor);
        request
    }

    pub(crate) async fn send<T: DeserializeOwned>(self) -> Result<Response<T>, Error> {
        let raw = self.execute().await?;
        let wire: ResponseWire<T> = decode(&raw)?;
        Ok(wire.into_response(raw.request_id, raw.billing))
    }

    pub(crate) async fn send_page<T: DeserializeOwned>(self) -> Result<Page<T>, Error> {
        let raw = self.execute().await?;
        let wire: ResponseWire<Vec<T>> = decode(&raw)?;
        Ok(Page::from_wire(wire, raw.request_id, raw.billing, self))
    }

    pub(crate) async fn send_fetch(self) -> Result<Response<FetchResource>, Error> {
        let raw = self.execute().await?;
        let wire: ResponseWire<serde_json::Value> = decode(&raw)?;
        let request_id = raw.request_id.clone();
        let mut response = wire.into_response(raw.request_id, raw.billing);
        let platform = response.platform.map(Platform::as_str).unwrap_or_default();
        let resource = response.resource.clone().unwrap_or_default();
        let data = std::mem::take(&mut response.data);
        let cause: Option<ErrorCause> = match FetchResource::decode(platform, &resource, data) {
            Ok(Some(data)) => return Ok(response.with_data(data)),
            Ok(None) => None,
            Err(cause) => Some(Arc::new(cause)),
        };
        let mut error = ApiError::new(
            "INVALID_RESPONSE",
            format!("Openhandle returned an unsupported {platform} {resource} resource."),
        );
        error.request_id = request_id;
        error.status = Some(200);
        error.cause = cause;
        Err(error.into())
    }

    async fn execute(&self) -> Result<RawResponse, Error> {
        let core = &self.client.core;
        let max_retries = self.controls.max_retries.unwrap_or(core.max_retries);
        let timeout = self
            .controls
            .timeout
            .filter(|timeout| !timeout.is_zero())
            .unwrap_or(core.timeout);
        let mut attempt = 0;
        loop {
            let error = match self.attempt(timeout).await {
                Ok(raw) => return Ok(raw),
                Err(error) => error,
            };
            if attempt >= max_retries || !error.retryable {
                return Err(Error::Api(error));
            }
            tokio::time::sleep(retry_delay(&error, attempt)).await;
            attempt += 1;
        }
    }

    async fn attempt(&self, timeout: Duration) -> Result<RawResponse, Box<ApiError>> {
        let core = &self.client.core;
        let mut target =
            ParsedUrl::parse(&format!("{}{}", core.base_url, self.path)).map_err(|cause| {
                let mut error =
                    ApiError::new("TRANSPORT_ERROR", "Openhandle request URL is invalid.");
                error.cause = Some(Arc::new(cause));
                Box::new(error)
            })?;
        if !self.query.0.is_empty() {
            target.query_pairs_mut().extend_pairs(
                self.query
                    .0
                    .iter()
                    .map(|(name, value)| (*name, value.as_str())),
            );
        }
        let mut request = core
            .http_client
            .request(self.method.clone(), target)
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", core.api_key))
            .header("X-OpenHandle-Client", CLIENT_HEADER)
            .timeout(timeout);
        if let Some(body) = &self.body {
            request = request
                .header(CONTENT_TYPE, "application/json")
                .body(body.clone());
        }

        let mut response = request.send().await.map_err(|cause| {
            let mut error = ApiError::new("TRANSPORT_ERROR", "Openhandle request failed.");
            error.retryable = !cause.is_timeout();
            error.cause = Some(Arc::new(cause));
            Box::new(error)
        })?;
        let status = response.status();
        let headers = response.headers().clone();
        let request_id = header(&headers, "X-Request-ID");

        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                        let mut error = ApiError::new(
                            "INVALID_RESPONSE",
                            "Openhandle response exceeded the maximum supported size.",
                        );
                        error.request_id = request_id;
                        error.status = Some(status.as_u16());
                        return Err(Box::new(error));
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(cause) => {
                    let mut error =
                        ApiError::new("INVALID_RESPONSE", "Openhandle response could not be read.");
                    error.request_id = request_id;
                    error.status = Some(status.as_u16());
                    error.retryable = true;
                    error.cause = Some(Arc::new(cause));
                    return Err(Box::new(error));
                }
            }
        }

        if !status.is_success() {
            return Err(Box::new(decode_api_error(status, &headers, &body)));
        }
        if body.is_empty() {
            body = b"{}".to_vec();
        }
        Ok(RawResponse {
            body,
            billing: billing_from_headers(&headers),
            request_id,
        })
    }
}

fn decode<T: DeserializeOwned>(raw: &RawResponse) -> Result<T, Error> {
    serde_json::from_slice(&raw.body).map_err(|cause| {
        let mut error = ApiError::new("INVALID_RESPONSE", "Openhandle returned invalid JSON.");
        error.request_id = raw.request_id.clone();
        error.status = Some(200);
        error.cause = Some(Arc::new(cause));
        error.into()
    })
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    error: ErrorBody,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ErrorBody {
    code: Option<String>,
    details: Option<serde_json::Map<String, serde_json::Value>>,
    message: Option<String>,
    request_id: Option<String>,
    retryable: Option<bool>,
}

fn decode_api_error(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> ApiError {
    let status_retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
    let fallback_message = format!("Openhandle request failed with status {}.", status.as_u16());
    let mut error = ApiError::new(format!("HTTP_{}", status.as_u16()), fallback_message);
    error.status = Some(status.as_u16());
    error.request_id = header(headers, "X-Request-ID");
    error.retry_after = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_retry_after);
    error.retryable = status_retryable;

    let envelope = match serde_json::from_slice::<ErrorEnvelope>(body) {
        Ok(envelope) => envelope,
        Err(cause) => {
            error.cause = Some(Arc::new(cause));
            return error;
        }
    };
    let ErrorBody {
        code,
        details,
        message,
        request_id,
        retryable,
    } = envelope.error;
    if let Some(code) = code.filter(|code| !code.is_empty()) {
        error.code = code;
    }
    if let Some(message) = message.filter(|message| !message.is_empty()) {
        error.message = message;
    }
    if let Some(request_id) = request_id.filter(|request_id| !request_id.is_empty()) {
        error.request_id = Some(request_id);
    }
    error.details = details;
    error.retryable = retryable.unwrap_or(false) || status_retryable;
    error
}

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn billing_from_headers(headers: &HeaderMap) -> Billing {
    Billing {
        cost: header(headers, "Openhandle-Cost"),
        dataset_version: header(headers, "Openhandle-Dataset-Version"),
        disposition: header(headers, "Openhandle-Billing-Disposition"),
        environment: header(headers, "Openhandle-Environment"),
        list_price: header(headers, "Openhandle-List-Price"),
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    let value = value.trim();
    let delay = match value.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() => Duration::from_secs_f64(seconds.max(0.0)),
        Ok(_) => return None,
        Err(_) => {
            let target = DateTime::parse_from_rfc2822(value).ok()?;
            let target = SystemTime::UNIX_EPOCH
                .checked_add(Duration::from_secs(u64::try_from(target.timestamp()).ok()?))?;
            target.duration_since(SystemTime::now()).unwrap_or_default()
        }
    };
    (!delay.is_zero()).then_some(delay)
}

fn retry_delay(error: &ApiError, attempt: u32) -> Duration {
    if let Some(retry_after) = error.retry_after {
        return retry_after;
    }
    let base = Duration::from_millis(250 * (1 << attempt.min(4))).min(Duration::from_secs(4));
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u32(attempt);
    let jitter = 0.75 + (hasher.finish() as f64 / u64::MAX as f64) * 0.5;
    base.mul_f64(jitter)
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
