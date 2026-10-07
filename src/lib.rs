//! The official Rust client for the Openhandle API. It reads public
//! Instagram, TikTok, X, and Reddit data.
//!
//! Every call follows one grammar:
//!
//! ```text
//! client.<platform>().<resource>(reference).<subresource>().<operation>(options)
//! ```
//!
//! Selecting a resource never sends a request. Only the terminal operations
//! `get`, `list`, `search`, and `fetch` do.
//!
//! ```no_run
//! use openhandle::{Client, Freshness, InstagramProfileOptions};
//!
//! # async fn run() -> Result<(), openhandle::Error> {
//! let client = Client::new(std::env::var("OPENHANDLE_TEST_KEY").unwrap_or_default())?;
//! let profile = client.instagram().profile("northstar_forge_test");
//!
//! let response = profile
//!     .get(InstagramProfileOptions::new().freshness(Freshness::TwentyFourHours))
//!     .await?;
//! println!("{:?} {:?}", response.data.handle, response.billing.cost);
//!
//! let mut posts = profile.posts().items(None);
//! while let Some(post) = posts.next().await {
//!     println!("{:?}", post?.id);
//! }
//! # Ok(())
//! # }
//! ```

mod client;
mod de;
mod errors;
mod generated;
mod options;
mod references;
mod response;
mod transport;

#[cfg(test)]
mod surface_tests;
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;

pub use client::{Client, ClientBuilder};
pub use errors::{ApiError, Error, ErrorCause, ReferenceError, ReferenceMismatchError};
pub use generated::resources::*;
pub use options::{FetchOptions, Freshness, Platform, RequestOptions, Source};
pub use references::{Id, ProfileReference, ResourceReference, Url, Username};
pub use response::{Billing, Items, Page, Response};

/// Response models generated from the OpenAPI component schemas.
pub mod models {
    pub use crate::generated::models::*;
}
