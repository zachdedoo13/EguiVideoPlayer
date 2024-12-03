use std::cell::LazyCell;
use std::path::Path;
use std::string::ToString;
use anyhow::Context;
use url::Url;

pub mod gstreamer_internals {
    pub mod player_backend;
    pub mod update;
    pub mod prober;
}


// helper functions
pub fn path_to_uri(path: &Path) -> anyhow::Result<String> {
    let url = Url::from_file_path(path).ok().context("Couldn't convert to uri")?;
    Ok(url.to_string())
}


// test_uris
pub const URI_ONLINE_CAR: LazyCell<String> = LazyCell::new(|| "http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/WhatCarCanYouGetForAGrand.mp4".to_string());
pub const URI_PATH_FRIEREN: LazyCell<String> = LazyCell::new(|| path_to_uri(Path::new("E:/TorrentArchive/AnimeLibary/frieren/ep_1_placeholder.mkv")).unwrap());