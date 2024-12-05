use crate::gstreamer_internals::prober::{Probe};
use crate::gstreamer_internals::update::Update;
use anyhow::Result;
use crossbeam_channel::{Receiver};
use gstreamer::prelude::{Cast, ElementExt, ElementExtManual, GstObjectExt, ObjectExt};
use gstreamer::{Caps, ClockTime, ElementFactory, FlowSuccess, Pipeline, SeekFlags, State};
use gstreamer_app::AppSink;
use std::time::Duration;
use std::thread::JoinHandle;

pub struct GstreamerBackend {
   pub uri: String,
   pub pipeline: Pipeline,
   pub appsink: AppSink,
   pub update_receiver: Receiver<Update>,
   pub probe: Option<Result<Probe>>,
   probe_future: Option<JoinHandle<Result<Probe>>>,
   force_frame_update: bool,
   pub is_paused: bool,
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

      let (update_sender, update_receiver) = crossbeam_channel::bounded::<Update>(2);

      appsink.set_callbacks(
         gstreamer_app::AppSinkCallbacks::builder()
             .new_sample(move |sink| {
                match sink.pull_sample() {
                   Ok(sample) => {
                      let update = Update::from_sample(sample).unwrap();
                      if update_sender.send_timeout(update, Duration::from_millis(500)).is_err() {
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
         is_paused: true,
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

   pub fn poll_update(&mut self) -> Result<Update> {
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
         match self.update_receiver.try_recv() {
            Ok(update) => {
               Ok(update)
            }
            Err(_) => {
               let (_, start_state, _) = self.pipeline.state(Some(ClockTime::from_mseconds(2)));
               if !matches!(start_state, State::Playing) { self.start()?; }
               let sample = self.appsink.pull_sample()?;
               let update = Update::from_sample(sample)?;
               if !matches!(start_state, State::Playing) { self.pipeline.set_state(start_state)?; }
               Ok(update)
            }
         }
      }
      else {
         Ok(self.update_receiver.try_recv()?)
      }
   }
}

/// Playback
impl GstreamerBackend {
   // state
   pub fn start(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Playing)?;
      self.is_paused = false;
      Ok(())
   }

   pub fn stop(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Paused)?;
      self.is_paused = true;
      Ok(())
   }

   pub fn exit(&mut self) -> Result<()> {
      self.pipeline.set_state(State::Null)?;
      self.is_paused = true;
      Ok(())
   }


   // seek
   pub fn seek_trickmode(&self, seek_to: ClockTime) -> Result<()> {
      let seek_flags =
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

   pub fn seek_normal(&self, seek_to: ClockTime) -> Result<()> {
      let seek_flags = SeekFlags::FLUSH | SeekFlags::KEY_UNIT;
      self.pipeline.seek_simple(seek_flags, seek_to)?;
      Ok(())
   }

   pub fn seek_nearest_keyframe(&self, current_time: ClockTime) -> Result<()> {
      let seek_flags = SeekFlags::KEY_UNIT;
      self.pipeline.seek_simple(seek_flags, current_time)?;
      Ok(())
   }


   // step
   pub fn step_frames(&self, _frames: i32) {
      todo!()
   }


   // poll
   pub fn queue_forced_update(&mut self) {
      self.force_frame_update = true;
   }

   pub fn force_update_now(&mut self, end_paused: bool) -> Result<Update> {
      self.start()?;
      let update = self.update_receiver.recv()?;
      if end_paused {
         self.stop()?;
      }
      Ok(update)
   }
}

/// Tracks
impl GstreamerBackend {

}
