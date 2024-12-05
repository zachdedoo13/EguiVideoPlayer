use anyhow::Result;
use gstreamer::tags::{AudioCodec, Bitrate, Title, VideoCodec};
use gstreamer::ClockTime;
use gstreamer_pbutils::prelude::DiscovererStreamInfoExt;
use gstreamer_pbutils::Discoverer;
use std::thread::JoinHandle;

// helper functions
#[derive(Debug)]
pub struct VideoStream {
   pub name: Option<String>,
   pub fps: Option<f64>,
   pub bitrate: Option<u32>,
   pub max_bitrate: Option<u32>,
   pub resolution: Option<(u32, u32)>,
   pub codec: Option<String>,
   pub index: Option<u32>,
}

#[derive(Debug)]
pub struct AudioStream {
   pub name: Option<String>,
   pub codec: Option<String>,
   pub bitrate: Option<u32>,
   pub index: Option<u32>,
}

#[derive(Debug)]
pub struct Probe {
   pub uri: String,
   pub captions: Vec<(Option<String>, usize)>,
   pub audio_streams: Vec<(AudioStream, usize)>,
   pub video_streams: Vec<(VideoStream, usize)>,
}
impl Probe {
   pub fn from_uri(uri: &str) -> Result<Probe> {
      let mut out = Probe {
         uri: uri.to_string(),
         captions: vec![],
         audio_streams: vec![],
         video_streams: vec![],
      };

      println!("Running discoverer");
      let discoverer = Discoverer::new(ClockTime::from_seconds(5))?;
      let info = discoverer.discover_uri(uri)?;

      for (i, video_stream) in info.video_streams().iter().enumerate() {
         let framerate = video_stream.framerate();
         let fps = Some(framerate.numer() as f64 / framerate.denom() as f64);
         let bitrate = Some(video_stream.bitrate());
         let max_bitrate = Some(video_stream.max_bitrate());
         let resolution = Some((video_stream.width(), video_stream.height()));
         let name = video_stream.tags().and_then(|t| t.get::<Title>().map(|f| f.get().to_string()));
         let codec = video_stream.tags().and_then(|t| t.get::<VideoCodec>().map(|f| f.get().to_string()));
         let index = video_stream.tags().and_then(|t| t.get::<gstreamer::tags::ContainerSpecificTrackId>().map(|f| f.get().to_string().parse::<u32>().ok())).flatten();

         let s_out = VideoStream {
            name,
            fps,
            bitrate,
            max_bitrate,
            resolution,
            codec,
            index,
         };

         out.video_streams.push((s_out, i));
      }

      for (i, subtitle_stream) in info.subtitle_streams().iter().enumerate() {
         if let Some(tags) = subtitle_stream.tags() {
            let language = tags.get::<Title>().map(|t| t.get().to_string());
            out.captions.push((language, i));
         }
      }

      for (i, audio_stream) in info.audio_streams().iter().enumerate() {
         if let Some(tags) = audio_stream.tags() {
            let name = tags.get::<Title>().map(|t| t.get().to_string());
            let codec = tags.get::<AudioCodec>().map(|t| t.get().to_string());
            let bitrate = tags.get::<Bitrate>().map(|t| t.get());
            let index = tags.get::<gstreamer::tags::ContainerSpecificTrackId>().map(|t| t.get().to_string().parse::<u32>().ok()).flatten();

            let a_out = AudioStream {
               name,
               codec,
               bitrate,
               index,
            };

            out.audio_streams.push((a_out, i));
         }
      }

      println!("Finished discoverer");

      Ok(out)
   }

   pub fn from_uri_future(uri: &str) -> JoinHandle<Result<Probe>> {
      let uri = uri.to_string();
      let handle = std::thread::spawn(move || {
         let res = Probe::from_uri(uri.as_str());
         res
      });
      handle
   }
}


#[cfg(test)]
mod tests {
   use super::*;
   use crate::{URI_ONLINE_CAR, URI_PATH_FRIEREN};

   #[test]
   fn probe_test_path() {
      gstreamer::init().unwrap();
      let uri = &*URI_ONLINE_CAR;
      let probe = Probe::from_uri(uri);
      println!("{probe:#?}");
      assert_eq!(probe.is_ok(), true);
   }

   #[test]
   fn probe_test_online() {
      gstreamer::init().unwrap();
      let uri = &*URI_PATH_FRIEREN;
      let probe = Probe::from_uri(uri);
      println!("{probe:#?}");
      assert_eq!(probe.is_ok(), true);
   }
}
