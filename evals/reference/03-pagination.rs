use openhandle::{Client, Error, InstagramProfilePostsPage};

pub async fn collect_post_ids(client: &Client) -> Result<Vec<String>, Error> {
    let mut ids = Vec::new();
    let mut page: InstagramProfilePostsPage = client
        .instagram()
        .profile("northstar_forge_test")
        .posts()
        .list(None)
        .await?;
    loop {
        ids.extend(page.data.iter().map(|post| post.id.clone()));
        match page.next().await? {
            Some(next) => page = next,
            None => return Ok(ids),
        }
    }
}
