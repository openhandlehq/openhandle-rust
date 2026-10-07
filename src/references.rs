use std::sync::Arc;

use reqwest::Url as ParsedUrl;

use crate::errors::{Error, ReferenceError, ReferenceMismatchError};

mod reddit;

#[cfg(test)]
mod conformance;
#[cfg(test)]
mod tests;

/// Selects a profile by username. A leading `@` is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Username(String);

impl Username {
    /// Creates a username reference.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Selects a resource by an opaque platform ID or native shorthand.
///
/// Platform IDs are strings. The SDK never infers an ID from a raw string.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Id(String);

impl Id {
    /// Creates an ID reference.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Selects a resource by a social URL. Its platform and resource are checked
/// locally, without a request.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Url(String);

impl Url {
    /// Creates a URL reference.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

macro_rules! string_conversions {
    ($($name:ident),*) => {
        $(
            impl From<&str> for $name {
                fn from(value: &str) -> Self {
                    Self(value.to_owned())
                }
            }

            impl From<String> for $name {
                fn from(value: String) -> Self {
                    Self(value)
                }
            }
        )*
    };
}

string_conversions!(Username, Id, Url);

/// A reference accepted by profile selectors.
///
/// A raw string is a username unless it is a supported social URL. Use
/// [`Username`], [`Id`], or [`Url`] to state the meaning explicitly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProfileReference {
    /// A username shorthand or a supported social URL.
    Raw(String),
    /// An explicit username.
    Username(String),
    /// An explicit platform ID.
    Id(String),
    /// An explicit social URL.
    Url(String),
}

/// A reference accepted by selectors whose shorthand is not a username.
///
/// A raw string is the resource's native ID or shortcode unless it is a
/// supported social URL. Use [`Id`] or [`Url`] to state the meaning
/// explicitly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ResourceReference {
    /// A native shorthand or a supported social URL.
    Raw(String),
    /// An explicit platform ID or native shorthand.
    Id(String),
    /// An explicit social URL.
    Url(String),
}

macro_rules! reference_conversions {
    ($reference:ident { $($variant:ident),* }) => {
        impl From<&str> for $reference {
            fn from(value: &str) -> Self {
                Self::Raw(value.to_owned())
            }
        }

        impl From<&String> for $reference {
            fn from(value: &String) -> Self {
                Self::Raw(value.clone())
            }
        }

        impl From<String> for $reference {
            fn from(value: String) -> Self {
                Self::Raw(value)
            }
        }

        $(
            impl From<$variant> for $reference {
                fn from(value: $variant) -> Self {
                    Self::$variant(value.0)
                }
            }
        )*
    };
}

reference_conversions!(ProfileReference { Username, Id, Url });
reference_conversions!(ResourceReference { Id, Url });

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    Raw(String),
    Username(String),
    Id(String),
    Url(String),
}

impl From<ProfileReference> for Reference {
    fn from(value: ProfileReference) -> Self {
        match value {
            ProfileReference::Raw(value) => Self::Raw(value),
            ProfileReference::Username(value) => Self::Username(value),
            ProfileReference::Id(value) => Self::Id(value),
            ProfileReference::Url(value) => Self::Url(value),
        }
    }
}

impl From<ResourceReference> for Reference {
    fn from(value: ResourceReference) -> Self {
        match value {
            ResourceReference::Raw(value) => Self::Raw(value),
            ResourceReference::Id(value) => Self::Id(value),
            ResourceReference::Url(value) => Self::Url(value),
        }
    }
}

struct SocialUrlResolution {
    identifier: String,
    platform: &'static str,
    resource: &'static str,
}

pub(crate) fn resolve_reference(
    reference: &Reference,
    platform: &str,
    resource: &str,
) -> Result<String, Error> {
    let (Reference::Raw(input)
    | Reference::Username(input)
    | Reference::Id(input)
    | Reference::Url(input)) = reference;
    let value = input.trim();
    if value.is_empty() {
        return Err(ReferenceError::new("reference values must not be empty").into());
    }

    match reference {
        Reference::Username(_) => {
            if resource != "profile" {
                return Err(ReferenceError::new(format!(
                    "the {resource} resource does not accept username references"
                ))
                .into());
            }
            username_reference(value, platform)
        }
        Reference::Id(_) => Ok(value.to_owned()),
        Reference::Url(_) => resolve_url_reference(value, platform, resource),
        Reference::Raw(_) => resolve_raw_reference(value, platform, resource),
    }
}

fn resolve_raw_reference(value: &str, platform: &str, resource: &str) -> Result<String, Error> {
    if looks_like_supported_social_url(value) {
        return resolve_url_reference(value, platform, resource);
    }
    if resource == "profile" {
        return username_reference(value, platform);
    }
    Ok(value.to_owned())
}

fn resolve_url_reference(value: &str, platform: &str, resource: &str) -> Result<String, Error> {
    let resolution = resolve_social_url(value)?;
    if resolution.platform != platform || resolution.resource != resource {
        return Err(ReferenceMismatchError {
            expected_platform: platform.to_owned(),
            expected_resource: resource.to_owned(),
            actual_platform: resolution.platform.to_owned(),
            actual_resource: resolution.resource.to_owned(),
        }
        .into());
    }
    Ok(resolution.identifier)
}

fn looks_like_supported_social_url(value: &str) -> bool {
    let lowered = value.trim().to_lowercase();
    let mut value = lowered.as_str();
    let has_scheme = value.contains("://");
    if let Some(index) = value.find("://") {
        value = &value[index + 3..];
    }
    let separator = value.find(['/', '?', '#']);
    if !has_scheme && separator.is_none() {
        return false;
    }
    let mut authority = match separator {
        Some(index) => &value[..index],
        None => value,
    };
    if let Some(index) = authority.rfind('@') {
        authority = &authority[index + 1..];
    }
    if let Some(index) = authority.rfind(':') {
        authority = &authority[..index];
    }
    supported_social_host(authority.strip_prefix("www.").unwrap_or(authority))
}

fn supported_social_host(host: &str) -> bool {
    matches!(
        host,
        "reddit.com"
            | "old.reddit.com"
            | "new.reddit.com"
            | "m.reddit.com"
            | "redd.it"
            | "instagram.com"
            | "tiktok.com"
            | "m.tiktok.com"
            | "vm.tiktok.com"
            | "vt.tiktok.com"
            | "x.com"
            | "twitter.com"
            | "mobile.twitter.com"
    )
}

fn username_reference(value: &str, platform: &str) -> Result<String, Error> {
    let username = value.strip_prefix('@').unwrap_or(value);
    let valid = match platform {
        "reddit" => is_reddit_name(username),
        "instagram" => is_instagram_name(username),
        "tiktok" => is_tiktok_name(username),
        "twitter" => is_twitter_name(username),
        _ => false,
    };
    if !valid {
        return Err(ReferenceError::new(format!("invalid {platform} username")).into());
    }
    Ok(format!("@{username}"))
}

fn resolve_social_url(input: &str) -> Result<SocialUrlResolution, Error> {
    let parsed = match ParsedUrl::parse(input) {
        Ok(parsed) if parsed.has_host() => Ok(parsed),
        _ => ParsedUrl::parse(&format!("https://{input}")),
    };
    let parsed = match parsed {
        Ok(parsed) if parsed.has_host() => parsed,
        Ok(_) => return Err(ReferenceError::new("invalid social URL").into()),
        Err(cause) => {
            return Err(ReferenceError {
                message: "invalid social URL".to_owned(),
                cause: Some(Arc::new(cause)),
            }
            .into());
        }
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(ReferenceError::new(
            "social URLs must use HTTP or HTTPS and cannot contain credentials",
        )
        .into());
    }

    let path =
        percent_decode(parsed.path()).ok_or_else(|| ReferenceError::new("invalid social URL"))?;
    if let Some(fragment) = parsed.fragment() {
        percent_decode(fragment).ok_or_else(|| ReferenceError::new("invalid social URL"))?;
    }
    let lowered = parsed.host_str().unwrap_or_default().to_lowercase();
    let host = lowered.strip_prefix("www.").unwrap_or(&lowered);
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if is_tiktok_short_link(host, &parts) {
        return Err(ReferenceError::new(
            "TikTok short links are not resolved locally. Use fetch instead.",
        )
        .into());
    }
    match host {
        "reddit.com" | "old.reddit.com" | "new.reddit.com" | "m.reddit.com" | "redd.it" => {
            reddit::resolve_reddit_url(host, &parts)
        }
        "instagram.com" => resolve_instagram_url(&parts),
        "tiktok.com" | "m.tiktok.com" => resolve_tiktok_url(&parts),
        "x.com" | "twitter.com" | "mobile.twitter.com" => resolve_twitter_url(&parts),
        _ => Err(ReferenceError::new(format!("unsupported social domain {host}")).into()),
    }
}

fn resolve_instagram_url(parts: &[&str]) -> Result<SocialUrlResolution, Error> {
    if let [kind, code] = parts {
        if ["p", "reel", "tv"].contains(kind) && is_shortcode(code) {
            return Ok(resolution("instagram", "post", code));
        }
    }
    if let ["stories", "highlights", id] = parts {
        if is_numeric_id(id) {
            return Ok(resolution("instagram", "highlight", id));
        }
    }
    if let ["stories", username, id] = parts {
        if is_instagram_name(username) && is_numeric_id(id) {
            return Ok(resolution("instagram", "story", id));
        }
    }
    if let [username] = parts {
        let reserved = [
            "p", "reel", "reels", "tv", "explore", "accounts", "direct", "stories",
        ];
        if is_instagram_name(username) && !reserved.contains(&username.to_lowercase().as_str()) {
            return Ok(resolution("instagram", "profile", &format!("@{username}")));
        }
    }
    Err(ReferenceError::new("unsupported Instagram URL").into())
}

fn is_tiktok_short_link(host: &str, parts: &[&str]) -> bool {
    if host == "vm.tiktok.com" || host == "vt.tiktok.com" {
        return parts.len() == 1 && is_shortcode(parts[0]);
    }
    (host == "tiktok.com" || host == "m.tiktok.com")
        && parts.len() == 2
        && parts[0] == "t"
        && is_shortcode(parts[1])
}

fn resolve_tiktok_url(parts: &[&str]) -> Result<SocialUrlResolution, Error> {
    let unsupported = || Err(ReferenceError::new("unsupported TikTok URL").into());
    let Some(first) = parts.first() else {
        return unsupported();
    };
    let Some(username) = first.strip_prefix('@') else {
        return unsupported();
    };
    if !is_tiktok_name(username) {
        return unsupported();
    }
    if let [_, "video", id] = parts {
        if is_numeric_id(id) {
            return Ok(resolution("tiktok", "post", id));
        }
    }
    if parts.len() == 1 {
        return Ok(resolution("tiktok", "profile", &format!("@{username}")));
    }
    unsupported()
}

fn resolve_twitter_url(parts: &[&str]) -> Result<SocialUrlResolution, Error> {
    if parts.len() >= 3
        && parts[1].eq_ignore_ascii_case("status")
        && is_twitter_name(parts[0])
        && is_numeric_id(parts[2])
    {
        return Ok(resolution("twitter", "post", parts[2]));
    }
    if let ["i", "web", "status", id] = parts {
        if is_numeric_id(id) {
            return Ok(resolution("twitter", "post", id));
        }
    }
    if let [username] = parts {
        let reserved = [
            "home",
            "explore",
            "search",
            "settings",
            "messages",
            "notifications",
            "i",
            "intent",
            "share",
        ];
        if is_twitter_name(username) && !reserved.contains(&username.to_lowercase().as_str()) {
            return Ok(resolution("twitter", "profile", &format!("@{username}")));
        }
    }
    Err(ReferenceError::new("unsupported Twitter URL").into())
}

fn resolution(
    platform: &'static str,
    resource: &'static str,
    identifier: &str,
) -> SocialUrlResolution {
    SocialUrlResolution {
        identifier: identifier.to_owned(),
        platform,
        resource,
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = (*bytes.get(index + 1)? as char).to_digit(16)?;
        let low = (*bytes.get(index + 2)? as char).to_digit(16)?;
        decoded.push((high * 16 + low) as u8);
        index += 3;
    }
    Some(String::from_utf8_lossy(&decoded).into_owned())
}

fn matches_class(
    value: &str,
    minimum: usize,
    maximum: usize,
    allowed: impl Fn(u8) -> bool,
) -> bool {
    (minimum..=maximum).contains(&value.len()) && value.bytes().all(allowed)
}

pub(crate) fn is_reddit_name(value: &str) -> bool {
    matches_class(value, 1, 100, |byte| {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
    })
}

fn is_numeric_id(value: &str) -> bool {
    matches_class(value, 1, usize::MAX, |byte| byte.is_ascii_digit())
}

fn is_instagram_name(value: &str) -> bool {
    matches_class(value, 1, 30, |byte| {
        byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_'
    })
}

fn is_tiktok_name(value: &str) -> bool {
    matches_class(value, 2, 24, |byte| {
        byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_'
    })
}

fn is_twitter_name(value: &str) -> bool {
    matches_class(value, 1, 15, |byte| {
        byte.is_ascii_alphanumeric() || byte == b'_'
    })
}

fn is_shortcode(value: &str) -> bool {
    matches_class(value, 1, usize::MAX, |byte| {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
    })
}
