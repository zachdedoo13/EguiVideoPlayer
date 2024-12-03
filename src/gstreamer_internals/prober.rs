use std::thread;
use anyhow::Result;
use gstreamer::tags::{AudioCodec, Bitrate, Title, VideoCodec};
use gstreamer::ClockTime;
use gstreamer_pbutils::prelude::DiscovererStreamInfoExt;
use gstreamer_pbutils::Discoverer;
use std::thread::JoinHandle;
use either::Either::{self, Left, Right};

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
         Probe::from_uri(uri.as_str())
      });
      handle
   }
}




pub enum PollRes<T> {
   NotInitialized,
   InProgress,
   Available(T),
   JustBecameAvailable(T),
}

pub struct TaskOrData<R: Send + 'static> {
   pub inner: Option<Either<R, JoinHandle<R>>>,
   pub data_has_been_checked: bool,
}

impl<R: Send + 'static> Default for TaskOrData<R> {
   fn default() -> Self {
      Self {
         inner: None,
         data_has_been_checked: false,
      }
   }
}

impl<R: Send + 'static> TaskOrData<R> {
   /// init with set data
   #[must_use]
   pub fn with_data(start_data: R) -> Self {
      Self {
         inner: Some(Left(start_data)),
         data_has_been_checked: false,
      }
   }

   /// init with no data, just routes to `TaskOrData::default`
   #[must_use]
   pub fn without_data() -> Self {
      Self::default()
   }

   pub fn start_task<F>(&mut self, func: F)
   where
       F: FnOnce() -> R + Send + 'static,
       R: Send + 'static,
   {
      // kill old processes
      if let Some(Right(_)) = &self.inner {
         // No need to abort, just drop the handle
         self.inner = None;
      }

      // start new processes
      let new: JoinHandle<R> = thread::spawn(func);

      self.inner = Some(Right(new));
   }

   /// checks if the future is done, returns an error if fails and sets the inner data back to none
   pub fn poll(&mut self) -> Result<(), Box<dyn std::any::Any + Send>> {
      if let Some(Right(in_progress)) = &mut self.inner {
         if in_progress.is_finished() {
            if let Some(join_handle) = self.inner.take() {
               if let Right(join_handle) = join_handle {
                  let fin: Result<R, Box<dyn std::any::Any + Send>> = join_handle.join();
                  return match fin {
                     Ok(fin_data) => {
                        self.inner = Some(Left(fin_data));
                        Ok(())
                     }
                     Err(e) => {
                        self.inner = None;
                        Err(e)
                     }
                  };
               }
            }
         }
      }
      Ok(())
   }

   pub fn check(&mut self) -> PollRes<&R> {
      match &self.inner {
         None => {
            self.data_has_been_checked = false;
            PollRes::NotInitialized
         }
         Some(either) => match either {
            Left(fin) => {
               if !self.data_has_been_checked {
                  self.data_has_been_checked = true;
                  PollRes::JustBecameAvailable(fin)
               } else {
                  PollRes::Available(fin)
               }
            }
               Right(_) => {
               self.data_has_been_checked = false;
               PollRes::InProgress
            }
         },
      }
   }

   pub fn check_mut(&mut self) -> PollRes<&mut R> {
      match &mut self.inner {
         None => PollRes::NotInitialized,
         Some(either) => match either {
            Left(fin) => PollRes::Available(fin),
            Right(_) => PollRes::InProgress,
         },
      }
   }

   pub fn is_running_task(&self) -> bool {
      if let Some(either) = &self.inner {
         return either.is_right();
      }
      false
   }

   pub fn is_init(&self) -> bool {
      self.inner.is_some()
   }

   pub fn reset(&mut self) {
      self.inner = None;
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
