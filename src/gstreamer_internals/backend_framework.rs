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


   //////////////////////
   // Subtitle Methods //
   //////////////////////

   fn toggle_subtitles(&mut self, set_to: bool) -> Result<()>;
}