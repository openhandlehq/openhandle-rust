use super::{Reference, resolve_reference};
use crate::errors::Error;

fn resolve(reference: Reference, platform: &str, resource: &str) -> Result<String, Error> {
    resolve_reference(&reference, platform, resource)
}

fn raw(value: &str) -> Reference {
    Reference::Raw(value.to_owned())
}

#[test]
fn profile_strings_resolve_to_usernames() {
    let cases = [
        (raw("openai"), "@openai"),
        (raw("@openai"), "@openai"),
        (raw("12356"), "@12356"),
        (raw("instagram.com"), "@instagram.com"),
        (Reference::Username("12356".to_owned()), "@12356"),
        (Reference::Id("25025320".to_owned()), "25025320"),
        (
            Reference::Url("instagram.com/openai/".to_owned()),
            "@openai",
        ),
        (raw("https://www.instagram.com/openai/"), "@openai"),
        (raw("instagram.com/openai/"), "@openai"),
        (raw("https://www.instagram.com/openai/?hl=en"), "@openai"),
    ];
    for (reference, expected) in cases {
        let resolved = resolve(reference.clone(), "instagram", "profile");
        assert_eq!(resolved.ok().as_deref(), Some(expected), "{reference:?}");
    }
}

#[test]
fn explicit_url_for_a_post_is_a_profile_mismatch() {
    let error = resolve(
        Reference::Url("https://www.instagram.com/p/Db04otPRpRH/".to_owned()),
        "instagram",
        "profile",
    )
    .unwrap_err();
    let Error::ReferenceMismatch(mismatch) = error else {
        panic!("expected a reference mismatch, got {error:?}");
    };
    assert_eq!(mismatch.actual_resource, "post");
    assert_eq!(mismatch.expected_resource, "profile");
}

#[test]
fn malformed_supported_url_does_not_fall_back_to_a_username() {
    let error = resolve(raw("instagram.com/%zz"), "instagram", "profile").unwrap_err();
    let Error::Reference(reference) = error else {
        panic!("expected a reference error, got {error:?}");
    };
    assert_eq!(reference.message, "invalid social URL");
}

#[test]
fn post_strings_resolve_to_shorthand_or_url_identifier() {
    assert_eq!(
        resolve(raw("Db04otPRpRH"), "instagram", "post").unwrap(),
        "Db04otPRpRH"
    );
    assert_eq!(
        resolve(raw("instagram.com/p/Db04otPRpRH/"), "instagram", "post").unwrap(),
        "Db04otPRpRH"
    );
}

#[test]
fn username_reference_is_rejected_for_a_post() {
    let error = resolve(
        Reference::Username("openai".to_owned()),
        "instagram",
        "post",
    )
    .unwrap_err();
    assert!(matches!(error, Error::Reference(_)), "{error:?}");
}

#[test]
fn url_with_credentials_is_rejected() {
    let error = resolve(
        raw("https://user:secret@www.instagram.com/openai/"),
        "instagram",
        "profile",
    )
    .unwrap_err();
    assert!(matches!(error, Error::Reference(_)), "{error:?}");
}

#[test]
fn instagram_story_and_highlight_urls_resolve_to_their_resource() {
    assert_eq!(
        resolve(
            raw("https://www.instagram.com/stories/highlights/17900000000000001/"),
            "instagram",
            "highlight"
        )
        .unwrap(),
        "17900000000000001"
    );
    assert_eq!(
        resolve(
            raw("https://www.instagram.com/stories/openai/3100000000000000001/"),
            "instagram",
            "story"
        )
        .unwrap(),
        "3100000000000000001"
    );
}

#[test]
fn twitter_web_status_url_resolves_to_a_post() {
    assert_eq!(
        resolve(
            raw("https://x.com/i/web/status/1890000000000000001"),
            "twitter",
            "post"
        )
        .unwrap(),
        "1890000000000000001"
    );
}

#[test]
fn reddit_comments_short_path_resolves_to_a_post() {
    assert_eq!(
        resolve(
            raw("https://www.reddit.com/comments/abc123/"),
            "reddit",
            "post"
        )
        .unwrap(),
        "abc123"
    );
}
