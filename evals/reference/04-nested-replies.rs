use openhandle::{Client, Error, Id, InstagramPostCommentRepliesPage};

pub async fn list_replies(client: &Client) -> Result<InstagramPostCommentRepliesPage, Error> {
    client
        .instagram()
        .post("Db04otPRpRH")
        .comment(Id::new("18120112390529134"))
        .replies()
        .list(None)
        .await
}
