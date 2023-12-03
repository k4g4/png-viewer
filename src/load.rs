use std::{io, path::PathBuf};

use iced::widget::canvas::Cache;
use tokio::{fs::File, sync::oneshot};

pub async fn load(path: PathBuf, cache_send: oneshot::Sender<Cache>) -> io::Result<()> {
    let file = File::open(path).await?;
    tracing::info!("{}", file.metadata().await.unwrap().len());

    cache_send
        .send(Cache::new())
        .map_err(|_| io::Error::other("failed to send cache"))
}
