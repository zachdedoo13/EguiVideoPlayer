use std::cell::LazyCell;
use std::path::Path;
use std::string::ToString;
use anyhow::Context;
use gstreamer::Fraction;
use url::Url;

pub mod gstreamer_internals {
    pub mod player_backend;
    pub mod update;
    pub mod prober;
    pub mod backend_framework;
    pub mod backend_v2;
}

pub mod gui {
    pub mod player;
}

pub mod wgpu {
    pub mod pack;
    pub mod display_texture;
}


// helper functions
pub fn path_to_uri(path: &Path) -> anyhow::Result<String> {
    let url = Url::from_file_path(path).ok().context("Couldn't convert to uri")?;
    Ok(url.to_string())
}

fn fraction_to_f64(fraction: Fraction) -> f64 {
    fraction.numer() as f64 / fraction.denom() as f64
}


// test_uris
pub const URI_ONLINE_CAR: LazyCell<String> = LazyCell::new(||
    "http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/WhatCarCanYouGetForAGrand.mp4".to_string());
pub const URI_PATH_FRIEREN: LazyCell<String> = LazyCell::new(||
    path_to_uri(Path::new("E:/TorrentArchive/AnimeLibary/frieren/[Judas] Sousou no Frieren - S01E01v2.mkv")).unwrap());

pub const URI_PATH_HELLS_PARADISE: LazyCell<String> = LazyCell::new(||
    path_to_uri(Path::new("E:/TorrentArchive/AnimeLibary/Jigokuraku S01 (BD 1080p AV1) [Dual-Audio] [MiniVodes]/Jigokuraku - S01E04 (BD 1080p AV1) [MiniVodes].mkv")).unwrap());


pub const URI_PATH_BROKO_BAD: LazyCell<String> = LazyCell::new(||
    path_to_uri(Path::new("E:/TorrentArchive/ShowLibary/Breaking.Bad.S01-S05.1080p.NF.WEB-DL.AV1.EAC3/Breaking Bad Season 2/Breaking.Bad.S02E01.Seven.Thirty-Seven.1080p.NF.WEB-DL.AV1.EAC3.mkv")).unwrap());


