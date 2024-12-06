use crate::fraction_to_f64;
use crate::gstreamer_internals::backend_framework::GstreamerBackendFramework;
use crate::gstreamer_internals::prober::Probe;
use crate::gstreamer_internals::update::FrameUpdate;
use anyhow::{bail, Result};
use crossbeam_channel::Receiver;
use gstreamer::prelude::{Cast, ElementExt, ElementExtManual, GstObjectExt, ObjectExt};
use gstreamer::{Caps, ClockTime, Element, ElementFactory, FlowSuccess, Pipeline, SeekFlags, SeekType, State};
use gstreamer_app::AppSink;
use gstreamer_video::VideoInfo;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct BackendV2 {
   pipeline: Pipeline,
   update_receiver: Receiver<(FrameUpdate, VideoInfo)>,

   probe: Result<Probe>,
   probe_future: Option<JoinHandle<Result<Probe>>>,

   latest_info: Option<VideoInfo>,
   latest_timecode: ClockTime,

   target_state: State,

   frame_queue_info: FrameQueueInfo,

   playback_speed: f64,
}

impl Drop for BackendV2 {
   fn drop(&mut self) {
      self.quit().unwrap()
   }
}

impl BackendV2 {
   fn handle_update(&mut self, inny: (FrameUpdate, VideoInfo)) -> FrameUpdate {
      self.latest_info = Some(inny.1);
      self.latest_timecode = inny.0.timecode;
      inny.0
   }
}

impl GstreamerBackendFramework for BackendV2 {
   fn init(uri: &str) -> Result<Self> {
      gstreamer::init()?;

      let pipeline: Pipeline = ElementFactory::make("playbin").build()?.dynamic_cast().unwrap();

      probe_props(&pipeline.clone().dynamic_cast().unwrap());

      // file
      pipeline.set_property("uri", uri);

      // video sink TODO hardware acc
      let appsink = ElementFactory::make("appsink")
          .name("videosink")
          .build()?
          .dynamic_cast::<AppSink>()
          .unwrap();


      let caps = Caps::builder("video/x-raw")
          .field("format", &"RGBA")
          .field("colorimetry", &"sRGB")
          .build();

      appsink.set_property("caps", &caps);
      pipeline.set_property("video-sink", &appsink);

      // settings


      // updater
      let (update_sender, update_receiver)
          = crossbeam_channel::bounded::<(FrameUpdate, VideoInfo)>(1);

      appsink.set_callbacks(
         gstreamer_app::AppSinkCallbacks::builder()
             .new_sample(move |sink| {
                match sink.pull_sample() {
                   Ok(sample) => {
                      let up_info = FrameUpdate::from_sample(sample).unwrap();
                      if update_sender.send_timeout(up_info, Duration::from_millis(500)).is_err() {
                         println!("Frame sender timed out 500ms");
                      }
                   }
                   Err(err) => {
                      panic!("{:?}", err);
                   }
                }

                Ok(FlowSuccess::Ok)
             })
             .build()
      );

      // debug info
      let bus = pipeline.bus().unwrap();
      std::thread::spawn(move || {
         for msg in bus.iter_timed(ClockTime::NONE) {
            use gstreamer::MessageView;

            match msg.view() {
               MessageView::Eos(..) => break,
               MessageView::Error(err) => {
                  println!(
                     "Error from {:?}: {} ({:?})",
                     err.src().map(|s| s.path_string()),
                     err.error(),
                     err.debug()
                  );
                  break;
               }
               _ => (),
            }
         }
         println!("Closing message bus for gstreamer backend");
      });

      let probe_future = Some(Probe::from_uri_future(uri));


      let mut this = Self {
         pipeline,
         update_receiver,
         probe: Err(anyhow::format_err!("Not initialized yet")),
         probe_future,
         latest_info: None,
         latest_timecode: ClockTime::ZERO,
         target_state: State::Null,
         frame_queue_info: FrameQueueInfo {
            queued: true, // renders at least one frame at start
            start_state: State::VoidPending,
            in_progress: false,
         },
         playback_speed: 1.0,
      };

      // ensures it starts in paused state
      this.stop()?;

      Ok(this)
   }

   fn update(&mut self) -> Result<FrameUpdate> {
      if self.probe_future.is_some() {
         let mut check = false;
         if let Some(fut) = &self.probe_future {
            check = fut.is_finished();
         }
         if check {
            let fut = self.probe_future.take().unwrap();
            let probe_res = fut.join().unwrap();
            self.probe = probe_res;
         }
      }

      match self.frame_queue_info.queued {
         true => {
            match self.frame_queue_info.in_progress {
               true => {
                  let upt = self.update_receiver.try_recv()?;

                  self.frame_queue_info.in_progress = false;
                  self.frame_queue_info.queued = false;

                  match self.frame_queue_info.start_state {
                     State::VoidPending | State::Null | State::Ready => {
                        println!("Attempted to set to undefined state");
                     }
                     State::Paused => { self.stop()?; }
                     State::Playing => { self.start()?; }
                  }

                  Ok(self.handle_update(upt))
               }
               false => {
                  self.frame_queue_info.in_progress = true;
                  self.frame_queue_info.start_state = self.get_predicted_state();

                  self.start()?;

                  // only continues if a frame was received
                  // self.seek_frames(1)?;

                  let upt = self.update_receiver.try_recv()?;

                  self.frame_queue_info.in_progress = false;
                  self.frame_queue_info.queued = false;

                  match self.frame_queue_info.start_state {
                     State::VoidPending | State::Null | State::Ready => {
                        println!("Attempted to set to undefined state");
                     }
                     State::Paused => { self.stop()?; }
                     State::Playing => { self.start()?; }
                  }

                  Ok(self.handle_update(upt))
               }
            }
         }
         false => {
            Ok(self.handle_update(self.update_receiver.try_recv()?))
         }
      }
   }

   //////////////////////
   // Playback Methods //
   //////////////////////

   fn start(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Playing)?;
      self.target_state = State::Playing;
      Ok(())
   }

   fn stop(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Paused)?;
      self.target_state = State::Paused;
      Ok(())
   }

   fn quit(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Null)?;
      self.target_state = State::Null;
      Ok(())
   }

   fn queue_frame_update(&mut self) {
      self.frame_queue_info.queued = true;
   }

   fn change_playback_speed(&mut self, speed: f64) -> Result<()> {
      let cp = self.latest_timecode;
      self.playback_speed = speed;
      self.pipeline.seek(
         speed,
         SeekFlags::FLUSH,
         SeekType::Set,
         cp,
         SeekType::None,
         ClockTime::NONE,
      )?;
      Ok(())
   }

   /////////////////////
   // Seeking Methods //
   /////////////////////

   fn seek_time(&mut self, seek_flags: SeekFlags, seek_to: ClockTime) -> Result<()> {
      self.pipeline.seek(
         self.playback_speed,
         seek_flags,
         SeekType::Set,
         seek_to,
         SeekType::None,
         ClockTime::NONE,
      )?;

      Ok(())
   }

   fn seek_timeline(&mut self, seek_to: ClockTime, accurate: bool) -> Result<()> {
      // self.pipeline.seek_simple(seek_flags, seek_to)?;
      if !self.frame_queue_info.in_progress {
         self.pipeline.seek(
            self.playback_speed,
            if accurate { SeekFlags::FLUSH } else { SeekFlags::FLUSH | SeekFlags::KEY_UNIT },
            SeekType::Set,
            seek_to,
            SeekType::None,
            ClockTime::NONE,
         )?;
      }

      Ok(())
   }

   fn seek_frames(&mut self, frames: i32) -> Result<()> {
      match frames {
         x if x == 0 => {
            panic!("Attempted to seek 0 frames for some reason, check logic")
         }

         // negative
         x if x < 0 => {
            let start_time = self.latest_timecode;
            let frametime = self.get_frametime();
            let back_sec = (start_time.seconds_f64() - (frametime * frames.abs() as f64)).max(0.0);
            let back_time = ClockTime::from_seconds_f64(back_sec);

            self.seek_time(SeekFlags::FLUSH, back_time)?;

            self.queue_frame_update();
            Ok(())
         }

         // positive non 0 or 1
         x if x > 0 => {
            let step_event = gstreamer::event::Step::new(
               gstreamer::format::Buffers::from_u64(frames as u64),
               1.0,
               true,
               false,
            );
            self.pipeline.send_event(step_event);
            self.queue_frame_update();
            Ok(())
         }

         _ => panic!("Something really fucked up if this gets called")
      }
   }

   //////////////////////
   // DataInfo Methods //
   //////////////////////

   fn get_frametime(&self) -> f64 {
      let frametime = if let Some(info) = self.get_latest_vidio_info() {
         let fps = fraction_to_f64(info.fps());
         1.0 / fps
      } else {
         1.0 / 30.0
      };

      frametime
   }

   fn get_probe(&self) -> Result<&Probe> {
      match self.probe.as_ref() {
         Ok(probe) => {
            Ok(probe)
         }
         Err(err) => {
            bail!("{err}")
         }
      }
      // self.probe
      // Ok(&self.probe?)
   }

   fn get_latest_vidio_info(&self) -> Option<&VideoInfo> {
      self.latest_info.as_ref()
   }

   fn current_playback_speed(&self) -> f64 {
      self.playback_speed
   }

   fn get_predicted_state(&self) -> State {
      self.target_state
   }

   fn timecode(&self) -> ClockTime {
      self.latest_timecode
   }

   fn get_duration(&self) -> Result<ClockTime> {
      let duration = self.pipeline.query_duration::<ClockTime>().unwrap_or(ClockTime::ZERO);
      Ok(duration)
   }

   ////////////////////
   // Stream Methods //
   ////////////////////

   fn get_sub_track(&self) -> Result<u32> {
      Ok(self.pipeline.property::<i32>("current-text") as u32)
   }
   fn set_sub_track(&mut self, track: u32) -> Result<()> {
      self.pipeline.set_property("current-text", track as i32);
      Ok(())
   }

   fn get_audio_track(&self) -> Result<u32> {
      Ok(self.pipeline.property::<i32>("current-audio") as u32)
   }
   fn set_audio_track(&mut self, track: u32) -> Result<()> {
      self.pipeline.set_property("current-audio", track as i32);
      Ok(())
   }

   fn get_video_track(&self) -> Result<u32> {
      Ok(self.pipeline.property::<i32>("current-video") as u32)
   }
   fn set_video_track(&mut self, track: u32) -> Result<()> {
      self.pipeline.set_property("current-video", track as i32);
      Ok(())
   }

   //////////////////////
   // Subtitle Methods //
   //////////////////////

   fn toggle_subtitles(&mut self, _set_to: bool) -> Result<()> {
      todo!()
   }
}

#[allow(dead_code)]
fn probe_props(element: &Element) {
   let props = element.list_properties();
   println!("\nProps of {}---------------", element.name().as_str());
   for prop in props {
      println!("Property: {} - Type: {:?}", prop.name(), prop.value_type())
   }

   println!("End-----------------------\n");
}

struct FrameQueueInfo {
   queued: bool,
   start_state: State,
   in_progress: bool,
}