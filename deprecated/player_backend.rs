use crate::gstreamer_internals::prober::{Probe};
use crate::gstreamer_internals::update::FrameUpdate;
use anyhow::{Result};
use crossbeam_channel::{Receiver};
use gstreamer::prelude::{Cast, ElementExt, ElementExtManual, GstObjectExt, ObjectExt};
use gstreamer::{Caps, ClockTime, ElementFactory, FlowSuccess, Fraction, Pipeline, SeekFlags, State};
use gstreamer_app::AppSink;
use std::time::{Duration, Instant};
use std::thread::JoinHandle;
use gstreamer_video::VideoInfo;

pub struct GstreamerBackend {
   pub uri: String,
   pub pipeline: Pipeline,
   pub appsink: AppSink,
   pub update_receiver: Receiver<(FrameUpdate, VideoInfo)>,
   pub probe: Option<Result<Probe>>,
   probe_future: Option<JoinHandle<Result<Probe>>>,
   force_frame_update: bool,

   pub latest_info: Option<VideoInfo>,
   pub timecode: ClockTime,
   pub target_state: State,
}

impl Drop for GstreamerBackend {
   fn drop(&mut self) {
      self.exit().unwrap();
   }
}

/// Constructors and update
impl GstreamerBackend {
   pub fn init(uri: &str) -> Result<Self> {
      gstreamer::init()?;

      let (pipeline, appsink) = Self::create_playbin_pipeline(&uri)?;
      pipeline.set_state(State::Paused)?;

      let (update_sender, update_receiver) = crossbeam_channel::bounded::<(FrameUpdate, VideoInfo)>(2);

      appsink.set_callbacks(
         gstreamer_app::AppSinkCallbacks::builder()
             .new_sample(move |sink| {
                match sink.pull_sample() {
                   Ok(sample) => {
                      let up_info = FrameUpdate::from_sample(sample).unwrap();
                      if update_sender.send_timeout(up_info, Duration::from_millis(500)).is_err() {
                         println!("Frame sender timed out");
                      }
                   }
                   Err(err) => {
                      panic!("{:?}", err);
                   }
                }

                Ok(FlowSuccess::Ok)
             }).build()
      );

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

      Ok(Self {
         uri: uri.to_string(),
         pipeline,
         appsink,
         update_receiver,
         probe: None,
         probe_future,
         force_frame_update: false,
         latest_info: None,
         timecode: ClockTime::ZERO,
         target_state: State::Paused,
      })
   }

   fn create_playbin_pipeline(uri: &str) -> Result<(Pipeline, AppSink)> {
      // init
      let pipeline: Pipeline = ElementFactory::make("playbin").build()?.dynamic_cast::<Pipeline>().unwrap();

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


      Ok((pipeline, appsink))
   }

   pub fn poll_update(&mut self) -> Result<FrameUpdate> {
      // update Probe
      if self.probe_future.is_some() {
         let mut check = false;
         if let Some(fut) = &self.probe_future {
            check = fut.is_finished();
         }
         if check {
            let fut = self.probe_future.take().unwrap();
            let probe_res = fut.join().unwrap();
            self.probe = Some(probe_res);
         }
      }

      // update frame
      if self.force_frame_update {
         self.force_frame_update = false;
         match self.try_get_update() {
            Ok(update) => {
               Ok(update)
            }
            Err(_) => {
               Ok(self.await_forced_update()?)
            }
         }
      }
      else {
         Ok(self.try_get_update()?)
      }
   }


   fn await_forced_update(&mut self) -> Result<FrameUpdate> {
      // let st = Instant::now();

      // let poll_state = Instant::now();
      let start_state = self.target_state;
      // if !matches!(start_state, State::Playing) { self.start()?; }
      self.start()?;
      // println!("\n\nForced frame time state_pull {:?}", poll_state.elapsed());

      // let poll_st = Instant::now();
      // let current_position = self.pipeline.query_position::<ClockTime>().unwrap_or(ClockTime::from_mseconds(0));
      // println!("Forced frame time poll {:?}", poll_st.elapsed());

      // let seek_st = Instant::now();
      // flush is required to make it not laggy but adds 20ms
      // self.pipeline.seek_simple(
      //    SeekFlags::FLUSH |
      //        SeekFlags::KEY_UNIT |
      //        SeekFlags::SNAP_AFTER,
      //    current_position)?; // TODO benchmark to see if theres a frame delay

      // self.step_frames_forward(1)?;
      // println!("Forced frame time seek {:?}", seek_st.elapsed());


      let update_st = Instant::now();
      let update = self.await_update()?;
      println!("Forced frame time rev {:?}", update_st.elapsed());

      // let state_st = Instant::now();

      match start_state {
         State::VoidPending => {}
         State::Null => {}
         State::Ready => {}
         State::Paused => {self.stop()?;}
         State::Playing => {self.start()?;}
      }
      // self.pipeline.set_state(start_state)?;
      // println!("Forced frame time state {:?}", state_st.elapsed());

      // println!("Forced frame time {:?}", st.elapsed());


      Ok(update)
   }

   fn try_get_update(&mut self) -> Result<FrameUpdate> {
      Ok(self.handle_update(self.update_receiver.try_recv()?))
   }

   fn await_update(&mut self) -> Result<FrameUpdate> {
      Ok(self.handle_update(self.update_receiver.recv()?))
   }

   fn handle_update(&mut self, inny: (FrameUpdate, VideoInfo)) -> FrameUpdate {
      self.latest_info = Some(inny.1);
      self.timecode = inny.0.timecode;
      inny.0
   }
}

/// Playback
impl GstreamerBackend {
   // state
   pub fn start(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Playing)?;
      self.target_state = State::Playing;
      Ok(())
   }

   pub fn stop(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Paused)?;
      self.target_state = State::Paused;
      Ok(())
   }

   pub fn exit(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Null)?;
      self.target_state = State::Null;
      Ok(())
   }

   pub fn is_paused(&self) -> bool {
      let _st = self.target_state;
      matches!(State::Paused, _st)
   }

   pub fn is_playing(&self) -> bool {
      let _st = self.target_state;
      matches!(State::Playing, _st)
   }


   // seek
   pub fn seek_trickmode(&self, seek_to: ClockTime) -> Result<()> {
      let seek_flags =
         SeekFlags::FLUSH |
          SeekFlags::TRICKMODE |
          SeekFlags::TRICKMODE_KEY_UNITS |
          SeekFlags::TRICKMODE_FORWARD_PREDICTED |
          SeekFlags::TRICKMODE_FORWARD_PREDICTED;

      self.pipeline.seek_simple(seek_flags, seek_to)?;
      Ok(())
   }

   pub fn seek_exact(&self, seek_to: ClockTime) -> Result<()> {
      let seek_flags = SeekFlags::ACCURATE;
      self.pipeline.seek_simple(seek_flags, seek_to)?;
      Ok(())
   }

   pub fn seek_keyframe(&self, seek_to: ClockTime) -> Result<()> {
      let seek_flags = SeekFlags::FLUSH | SeekFlags::KEY_UNIT;
      self.pipeline.seek_simple(seek_flags, seek_to)?;
      Ok(())
   }

   pub fn seek_just_flush(&self, seek_to: ClockTime) -> Result<()> {
      let seek_flags = SeekFlags::FLUSH;
      self.pipeline.seek_simple(seek_flags, seek_to)?;
      Ok(())
   }

   pub fn seek_nearest_keyframe(&self, current_time: ClockTime) -> Result<()> {
      let seek_flags = SeekFlags::KEY_UNIT;
      self.pipeline.seek_simple(seek_flags, current_time)?;
      Ok(())
   }


   // step
   pub fn step_frames_forward_exact(&mut self, frames: u64) -> Result<()> {
      if let Some(info) = &self.latest_info {
         let seek_flags = SeekFlags::FLUSH;
         let current_position = self.pipeline.query_position::<ClockTime>().unwrap_or(ClockTime::from_mseconds(0));
         let fps = fraction_to_f64(info.fps());
         let frame_duration = ClockTime::from_seconds_f64((1.0 / fps) - 0.01); // Assuming 30 FPS, adjust as needed
         let seek_to = current_position + frame_duration * frames;
         self.pipeline.seek_simple(seek_flags, seek_to)?;

      } else {
         self.queue_forced_update();
      }

      Ok(())
   }

   pub fn get_duration(&self) -> Result<ClockTime> {
      let duration = self.pipeline.query_duration::<ClockTime>().unwrap_or(ClockTime::ZERO);
      Ok(duration)
   }

   // poll
   pub fn queue_forced_update(&mut self) {
      self.force_frame_update = true;
   }
}

/// Tracks
impl GstreamerBackend {

}


fn fraction_to_f64(fraction: Fraction) -> f64 {
   fraction.numer() as f64 / fraction.denom() as f64
}