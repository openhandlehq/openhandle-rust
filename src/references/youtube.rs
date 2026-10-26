use reqwest::Url as ParsedUrl;

use crate::errors::{Error, ReferenceError};

use super::{SocialUrlResolution, matches_class, resolution};

pub(super) fn resolve_youtube_url(
    host: &str,
    parsed: &ParsedUrl,
    parts: &[&str],
) -> Result<SocialUrlResolution, Error> {
    if host == "youtu.be" {
        if let [id] = parts {
            if is_youtube_post_id(id) {
                return Ok(resolution("youtube", "post", id));
            }
        }
    }
    if let ["watch"] = parts {
        let video = parsed
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        if is_youtube_post_id(&video) {
            return Ok(resolution("youtube", "post", &video));
        }
    }
    if let [kind, id] = parts {
        if ["shorts", "live", "embed"].contains(kind) && is_youtube_post_id(id) {
            return Ok(resolution("youtube", "post", id));
        }
    }
    if let Some(handle) = parts.first().and_then(|first| first.strip_prefix('@')) {
        if is_youtube_handle(handle) {
            return Ok(resolution("youtube", "profile", &format!("@{handle}")));
        }
    }
    if parts.len() >= 2 && parts[0] == "channel" && is_youtube_profile_id(parts[1]) {
        return Ok(resolution("youtube", "profile", parts[1]));
    }
    Err(ReferenceError::new("unsupported YouTube URL").into())
}

pub(crate) fn is_youtube_handle(value: &str) -> bool {
    (1..=100).contains(&value.chars().count())
        && value.chars().all(|character| {
            character.is_alphabetic()
                || character.is_numeric()
                || ['.', '_', '-'].contains(&character)
        })
}

fn is_youtube_post_id(value: &str) -> bool {
    matches_class(value, 11, 11, is_youtube_id_byte)
}

fn is_youtube_profile_id(value: &str) -> bool {
    value
        .strip_prefix("UC")
        .is_some_and(|rest| matches_class(rest, 22, 22, is_youtube_id_byte))
}

fn is_youtube_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}
