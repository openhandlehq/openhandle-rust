use crate::errors::{Error, ReferenceError};

use super::{SocialUrlResolution, is_reddit_name, resolution};

pub(super) fn resolve_reddit_url(host: &str, parts: &[&str]) -> Result<SocialUrlResolution, Error> {
    if host == "redd.it" {
        if let [id] = parts {
            if is_reddit_post_id(id) {
                return Ok(resolution("reddit", "post", id));
            }
        }
    }
    if let [kind, username] = parts {
        if (*kind == "u" || *kind == "user") && is_reddit_name(username) {
            return Ok(resolution("reddit", "profile", &format!("@{username}")));
        }
        if *kind == "r" && is_reddit_name(username) {
            return Ok(resolution("reddit", "subreddit", username));
        }
    }
    if parts.len() >= 4 && parts[0] == "r" && parts[2] == "comments" && is_reddit_post_id(parts[3])
    {
        return Ok(resolution("reddit", "post", parts[3]));
    }
    if parts.len() >= 2 && parts[0] == "comments" && is_reddit_post_id(parts[1]) {
        return Ok(resolution("reddit", "post", parts[1]));
    }
    Err(ReferenceError::new("unsupported Reddit URL").into())
}

fn is_reddit_post_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}
