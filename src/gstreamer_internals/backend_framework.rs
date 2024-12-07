use std::ops::RangeInclusive;
use anyhow::Result;
use gstreamer::{ClockTime, SeekFlags, State};
use gstreamer_video::VideoInfo;
use crate::gstreamer_internals::prober::Probe;
use crate::gstreamer_internals::update::FrameUpdate;

pub trait GstreamerBackendFramework: Sized {
   fn init(uri: &str) -> Result<Self>;

   fn update(&mut self) -> Result<FrameUpdate>;


   //////////////////////
   // Playback Methods //
   //////////////////////

   fn start(&mut self) -> Result<()>;
   fn stop(&mut self) -> Result<()>;
   fn quit(&mut self) -> Result<()>;

   fn queue_frame_update(&mut self);

   fn change_playback_speed(&mut self, speed: f64) -> Result<()>;

   /////////////////////
   // Seeking Methods //
   /////////////////////

   fn seek_time(&mut self, seek_flags: SeekFlags, seek_to: ClockTime) -> Result<()>;

   fn seek_timeline(&mut self, seek_to: ClockTime, accurate: bool) -> Result<()>;

   fn seek_frames(&mut self, frames: i32) -> Result<()>;

   //////////////////////
   // DataInfo Methods //
   //////////////////////

   fn get_frametime(&self) -> f64;

   fn is_playing(&self) -> bool {
      let _st = self.get_predicted_state();
      matches!(State::Playing, _st)
   }

   fn is_paused(&self) -> bool {
      let _st = self.get_predicted_state();
      matches!(State::Paused, _st)
   }

   fn get_probe(&self) -> Result<&Probe>;

   fn get_latest_vidio_info(&self) -> Option<&VideoInfo>;

   fn current_playback_speed(&self) -> f64;

   fn get_predicted_state(&self) -> State;

   fn timecode(&self) -> ClockTime;

   fn get_duration(&self) -> Result<ClockTime>;

   ////////////////////
   // Stream Methods //
   ////////////////////

   fn get_sub_track(&self) -> Result<u32>;
   fn set_sub_track(&mut self, track: u32) -> Result<()>;

   fn get_audio_track(&self) -> Result<u32>;
   fn set_audio_track(&mut self, track: u32) -> Result<()>;

   fn get_video_track(&self) -> Result<u32>;
   fn set_video_track(&mut self, track: u32) -> Result<()>;

   fn set_audio_device(&mut self, device: &str) -> Result<()>;
   fn list_audio_devices(&self) -> Result<Vec<(String, String)>>;
   fn get_current_audio_device(&self) -> Option<String>;

   fn get_current_volume(&self) -> f64;
   fn get_volume_range(&self) -> RangeInclusive<f64>;
   fn set_volume(&mut self, to: f64) -> Result<()>;

   //////////////////////
   // Subtitle Methods //
   //////////////////////

   fn toggle_playflag(&mut self, set_to: bool, flag: u32) -> Result<()>;

   fn get_playflag_state(&self, flag: u32) -> Result<bool>;

}

pub struct PlayFlags;
impl PlayFlags {
   pub const VIDEO: u32 = 1 << 0;
   pub const AUDIO: u32 = 1 << 1;
   pub const SUBTITLES: u32 = 1 << 2;
   pub const VIS: u32 = 1 << 3;
   pub const SOFT_VOLUME: u32 = 1 << 4;
   pub const NATIVE_AUDIO: u32 = 1 << 5;
   pub const NATIVE_VIDEO: u32 = 1 << 6;
   pub const DOWNLOAD: u32 = 1 << 7;
   pub const BUFFERING: u32 = 1 << 8;
   pub const DEINTERLACE: u32 = 1 << 9;
   pub const SOFT_COLORBALANCE: u32 = 1 << 10;
   pub const FORCE_FILTERS: u32 = 1 << 11;
   pub const FORCE_SW_DECODERS: u32 = 1 << 12;
}
