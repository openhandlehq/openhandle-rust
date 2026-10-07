use std::collections::VecDeque;
use std::fmt;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::{Context, Poll};

use chrono::{DateTime, Utc};
use futures_core::Stream;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::errors::Error;
use crate::generated::models::ResponseMeta;
use crate::options::{Platform, Source};
use crate::transport::OperationRequest;

/// Accounting metadata returned in response headers.
///
/// Money values stay decimal strings, so no precision is lost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Billing {
    /// Authoritative charge for this request, such as `0.0005`.
    pub cost: Option<String>,
    /// Synthetic dataset version. Present for Test keys.
    pub dataset_version: Option<String>,
    /// How the request was accounted for, such as `allowance` or `prepaid`.
    pub disposition: Option<String>,
    /// Environment selected by the API key: `test` or `live`.
    pub environment: Option<String>,
    /// Live-equivalent list price for the source and freshness that answered.
    pub list_price: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponseWire<T> {
    data: T,
    #[serde(default)]
    platform: Option<Platform>,
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    captured_at: Option<DateTime<Utc>>,
    #[serde(default)]
    source: Option<Source>,
    #[serde(default)]
    meta: Option<ResponseMeta>,
}

impl<T> ResponseWire<T> {
    pub(crate) fn into_response(self, request_id: Option<String>, billing: Billing) -> Response<T> {
        Response {
            data: self.data,
            platform: self.platform,
            resource: self.resource,
            captured_at: self.captured_at,
            source: self.source,
            meta: self.meta,
            request_id,
            billing,
        }
    }
}

/// A typed response from a `get`, `fetch`, or unpaginated `list` operation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Response<T> {
    /// The returned resource.
    pub data: T,
    /// Platform of the resource.
    pub platform: Option<Platform>,
    /// Resource family, such as `profile` or `post`.
    pub resource: Option<String>,
    /// When the data was read from the platform.
    pub captured_at: Option<DateTime<Utc>>,
    /// Whether the answer is `live` or from the `cache`.
    pub source: Option<Source>,
    /// Operation metadata returned with the response.
    pub meta: Option<ResponseMeta>,
    /// Request identifier to include in logs and support requests.
    pub request_id: Option<String>,
    /// Accounting metadata from the response headers.
    pub billing: Billing,
}

impl<T> Response<T> {
    pub(crate) fn with_data<U>(self, data: U) -> Response<U> {
        Response {
            data,
            platform: self.platform,
            resource: self.resource,
            captured_at: self.captured_at,
            source: self.source,
            meta: self.meta,
            request_id: self.request_id,
            billing: self.billing,
        }
    }
}

/// One typed page returned by a paginated `list` or `search` operation.
#[derive(Clone)]
#[non_exhaustive]
pub struct Page<T> {
    /// Items on this page.
    pub data: Vec<T>,
    /// Pagination and operation metadata.
    pub meta: ResponseMeta,
    /// Platform of the items.
    pub platform: Option<Platform>,
    /// Resource family of the items.
    pub resource: Option<String>,
    /// When the data was read from the platform.
    pub captured_at: Option<DateTime<Utc>>,
    /// Whether the answer is `live` or from the `cache`.
    pub source: Option<Source>,
    /// Request identifier to include in logs and support requests.
    pub request_id: Option<String>,
    /// Accounting metadata from the response headers.
    pub billing: Billing,
    request: OperationRequest,
}

impl<T> Page<T> {
    pub(crate) fn from_wire(
        wire: ResponseWire<Vec<T>>,
        request_id: Option<String>,
        billing: Billing,
        request: OperationRequest,
    ) -> Self {
        Self {
            data: wire.data,
            meta: wire.meta.unwrap_or_default(),
            platform: wire.platform,
            resource: wire.resource,
            captured_at: wire.captured_at,
            source: wire.source,
            request_id,
            billing,
            request,
        }
    }

    /// Returns the opaque cursor of the next page, or `None` at the end.
    pub fn next_cursor(&self) -> Option<&str> {
        self.meta
            .cursors
            .next
            .as_deref()
            .filter(|cursor| !cursor.is_empty())
    }

    /// Reports whether the API returned another cursor.
    pub fn has_next_page(&self) -> bool {
        self.next_cursor().is_some()
    }
}

impl<T: DeserializeOwned> Page<T> {
    /// Requests the next page with the same operation and options.
    ///
    /// Returns `Ok(None)` without a request when there is no next cursor.
    pub async fn next(&self) -> Result<Option<Page<T>>, Error> {
        let Some(cursor) = self.next_cursor() else {
            return Ok(None);
        };
        self.request.with_cursor(cursor).send_page().await.map(Some)
    }
}

impl<T: fmt::Debug> fmt::Debug for Page<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Page")
            .field("data", &self.data)
            .field("meta", &self.meta)
            .field("platform", &self.platform)
            .field("resource", &self.resource)
            .field("captured_at", &self.captured_at)
            .field("source", &self.source)
            .field("request_id", &self.request_id)
            .field("billing", &self.billing)
            .finish_non_exhaustive()
    }
}

type PageFuture<T> = Pin<Box<dyn Future<Output = Result<Page<T>, Error>> + Send>>;

/// A lazy stream over every item of a paginated operation.
///
/// It requests one page at a time, only when the previous page is used up.
/// It never retries a cursor sequence on its own: after an error, such as
/// `UPSTREAM_SWITCHED`, it ends.
#[must_use = "items does nothing until you call next or poll it"]
pub struct Items<T> {
    next_request: Option<Result<OperationRequest, Error>>,
    pending: Option<PageFuture<T>>,
    buffer: VecDeque<T>,
}

impl<T> Unpin for Items<T> {}

impl<T: DeserializeOwned + Send + 'static> Items<T> {
    pub(crate) fn new(request: Result<OperationRequest, Error>) -> Self {
        Self {
            next_request: Some(request),
            pending: None,
            buffer: VecDeque::new(),
        }
    }

    /// Returns the next item, requesting a page only when needed.
    ///
    /// Returns `None` after the last item or after an error.
    #[allow(clippy::should_implement_trait)]
    pub async fn next(&mut self) -> Option<Result<T, Error>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }
}

impl<T: DeserializeOwned + Send + 'static> Stream for Items<T> {
    type Item = Result<T, Error>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(item) = this.buffer.pop_front() {
                return Poll::Ready(Some(Ok(item)));
            }
            if let Some(pending) = this.pending.as_mut() {
                let result = match pending.as_mut().poll(context) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(result) => result,
                };
                this.pending = None;
                match result {
                    Ok(page) => {
                        this.next_request = page
                            .next_cursor()
                            .map(|cursor| Ok(page.request.with_cursor(cursor)));
                        this.buffer.extend(page.data);
                        continue;
                    }
                    Err(error) => {
                        this.next_request = None;
                        return Poll::Ready(Some(Err(error)));
                    }
                }
            }
            match this.next_request.take() {
                None => return Poll::Ready(None),
                Some(Err(error)) => return Poll::Ready(Some(Err(error))),
                Some(Ok(request)) => this.pending = Some(Box::pin(request.send_page())),
            }
        }
    }
}

impl<T> fmt::Debug for Items<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Items")
            .field("buffered", &self.buffer.len())
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}
