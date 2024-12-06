use crate::gstreamer_internals::backend_framework::GstreamerBackendFramework;
use crate::gstreamer_internals::prober::Probe;
use crate::gstreamer_internals::update::FrameUpdate;
use anyhow::{bail, Result};
use crossbeam_channel::Receiver;
use gstreamer::prelude::{Cast, ElementExt, ElementExtManual, GstObjectExt, ObjectExt};
use gstreamer::{Caps, ClockTime, ElementFactory, FlowSuccess, Pipeline, SeekFlags, SeekType, State};
use gstreamer_app::AppSink;
use gstreamer_video::VideoInfo;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct BackendV2 {
   pipeline: Pipeline,
   appsink: AppSink,
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

      // file
      pipeline.set_property("uri", uri);

      // sink TODO hardware acc
      let appsink = ElementFactory::make("appsink")
          .name("videosink")
          .build()?
          .dynamic_cast::<AppSink>()
          .unwrap();


      let caps = Caps::builder("video/x-raw")
          .field("format", &"RGBA")
          .field("colorimetry", &"sRGB")
          .build();

      // settings
      appsink.set_property("caps", &caps);
      pipeline.set_property("video-sink", &appsink);

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
         appsink,
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
                     State::Paused => {self.stop()?;}
                     State::Playing => {self.start()?;}
                  }

                  Ok(self.handle_update(upt))
               }
               false => {
                  self.frame_queue_info.in_progress = true;
                  self.frame_queue_info.start_state = self.get_predicted_state();

                  self.start()?;

                  // only continues if a frame was received
                  let upt = self.update_receiver.try_recv()?;

                  self.frame_queue_info.in_progress = false;
                  self.frame_queue_info.queued = false;

                  match self.frame_queue_info.start_state {
                     State::VoidPending | State::Null | State::Ready => {
                        println!("Attempted to set to undefined state");
                     }
                     State::Paused => {self.stop()?;}
                     State::Playing => {self.start()?;}
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

   fn seek_time(&mut self, seek_flags: SeekFlags, seek_to: ClockTime) -> Result<()> {
      self.pipeline.seek_simple(seek_flags, seek_to)?;
      Ok(())
   }

   fn seek_frames(&mut self, frames: i32) -> Result<()> {
      match frames {
         x if x == 0 => {
            panic!("Attempted to seek 0 frames for some reason, check logic")
         }

         x if x == 1 => {
            self.queue_frame_update();
            Ok(())
         }

         x if x == -1 => {
            todo!()
         }

         // negative non 0 or -1
         x if x < 0 => {
            todo!()
         }

         // positive non 0 or 1
         x if x > 0 => {
            todo!()
         }

         _ => panic!("Something really fucked up if this gets called")
      }
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

   fn current_playback_speed(&self) -> f64 {
      self.playback_speed
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

   fn get_latest_vidio_info(&mut self) -> Option<&VideoInfo> {
      self.latest_info.as_ref()
   }
}



struct FrameQueueInfo {
   queued: bool,
   start_state: State,
   in_progress: bool,
}