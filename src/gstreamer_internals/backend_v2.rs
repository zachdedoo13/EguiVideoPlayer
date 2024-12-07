use crate::fraction_to_f64;
use crate::gstreamer_internals::backend_framework::GstreamerBackendFramework;
use crate::gstreamer_internals::prober::Probe;
use crate::gstreamer_internals::update::FrameUpdate;
use anyhow::{bail, Context, Result};
use crossbeam_channel::Receiver;
use gstreamer::ffi::GstObject;
use gstreamer::glib::gobject_ffi::{g_object_get, g_object_set, GObject};
use gstreamer::glib::translate::ToGlibPtr;
use gstreamer::glib::ParamFlags;
use gstreamer::prelude::{Cast, ElementExt, ElementExtManual, GstBinExtManual, GstObjectExt, IsA, ObjectExt};
use gstreamer::{Bin, Caps, ClockTime, Element, ElementFactory, FlowSuccess, Object, Pipeline, SeekFlags, SeekType, State};
use gstreamer_app::AppSink;
use gstreamer_video::glib::Value;
use gstreamer_video::VideoInfo;
use std::ffi::CString;
use std::ops::RangeInclusive;
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

   volume: Element,
   current_volume: f64,
   audio_sink: Element,
   current_audio_device: Option<String>,
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

   fn make_audio_sink(device: Option<&str>) -> Result<(Bin, Element, Element)> {
      // Create a new Bin
      let bin = Bin::new();

      // Create elements
      let audio_convert = ElementFactory::make("audioconvert").build()?;
      let audio_resample = ElementFactory::make("audioresample").build()?;
      let volume = ElementFactory::make("volume").build()?;

      #[cfg(target_os = "windows")]
      let audio_sink = ElementFactory::make("wasapisink")
          .name("audio-sink")
          .build()?;

      #[cfg(not(target_os = "windows"))]
      let audio_sink = ElementFactory::make("autoaudiosink")
          .name("audio-sink")
          .build()?;

      #[cfg(target_os = "windows")]
      if let Some(device) = device {
         audio_sink.set_property("device", device);
      }

      probe_props(&audio_sink);
      probe_props(&volume);

      // Add elements to the Bin
      bin.add_many(&[&audio_convert, &audio_resample, &volume, &audio_sink])?;

      // Link elements together
      Element::link_many(&[&audio_convert, &audio_resample, &volume, &audio_sink])?;

      // Add a ghost pad to the Bin to expose the audio_convert's sink pad
      let ghost_pad = gstreamer::GhostPad::with_target(
         &audio_convert.static_pad("sink").unwrap()
      )?;
      bin.add_pad(&ghost_pad)?;

      Ok((bin, volume, audio_sink))
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


      // audio sink

      let (audio_bin, volume, audio_sink) = Self::make_audio_sink(None)?;
      pipeline.set_property("audio-sink", &audio_bin);

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
         volume,
         current_volume: 2.5,
         audio_sink,
         current_audio_device: None,
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

   fn set_audio_device(&mut self, device: &str) -> Result<()> {
      #[cfg(target_os = "windows")]
      {
         // Set the pipeline state to NULL
         self.pipeline.set_state(State::Null)?;

         // Remove the current audio-sink
         self.pipeline.set_property("audio-sink", None::<&Element>);

         // Create a new audio-sink
         let (new_audio_bin, new_volume, new_audio_sink) = Self::make_audio_sink(Some(device))?;

         // Set the new audio-sink to the pipeline
         self.pipeline.set_property("audio-sink", &new_audio_bin);

         // Update the audio_sink and volume fields
         self.audio_sink = new_audio_sink;
         self.volume = new_volume;

         // Set the pipeline state back to PLAYING or the desired state
         self.pipeline.set_state(self.target_state)?;

         // wait till state change is successful
         let _ = self.pipeline.state(ClockTime::MAX);

         self.seek_time(SeekFlags::FLUSH | SeekFlags::ACCURATE, self.latest_timecode)?;

         println!("Audio device change success");
         self.current_audio_device = Some(device.to_string());

         Ok(())
      }

      #[cfg(not(target_os = "windows"))]
      {
         // println!("Set audio device only works on windows");
         // bail!("Set audio device only works on windows");

         compile_error!("Set audio device only works on windows")
      }
   }

   fn list_audio_devices(&self) -> Result<Vec<(String, String)>> {
      #[cfg(target_os = "windows")]
      {
         let mut out = vec![];

         let device_collection = wasapi::DeviceCollection::new(&wasapi::Direction::Render).ok().context("Couldn't get collection")?;
         for res in device_collection.into_iter() {
            if let Ok(device) = res {
               let name = device.get_friendlyname().ok().context("Couldn't get friendly name")?;
               let id = device.get_id().ok().context("Couldn't get friendly id")?;
               out.push((name, id));
            }
         }

         Ok(out)
      }

      #[cfg(not(target_os = "windows"))]
      {
         // println!("Set audio device only works on windows");
         // bail!("Set audio device only works on windows");

         compile_error!("List audio devices only works on windows")
      }
   }

   fn get_current_audio_device(&self) -> Option<String> {
      self.current_audio_device.clone()
   }

   fn get_current_volume(&self) -> f64 {
      self.current_volume
   }

   fn get_volume_range(&self) -> RangeInclusive<f64> {
      0.0..=5.0
   }

   fn set_volume(&mut self, to: f64) -> Result<()> {
      self.current_volume = to;
      self.volume.set_property("volume", to);
      Ok(())
   }

   //////////////////////
   // Subtitle Methods //
   //////////////////////

   fn toggle_playflag(&mut self, set_to: bool, flag: u32) -> Result<()> {
      let gobject_ptr = to_g_obj_pointer(self.pipeline.clone())?;

      let property_name = CString::new("flags")?;
      let mut flags: u32 = 0;

      unsafe {
         g_object_get(gobject_ptr, property_name.as_ptr(), &mut flags as *mut u32 as *mut _, std::ptr::null::<i32>());

         if set_to {
            flags |= flag;
         } else {
            flags &= !flag;
         }

         g_object_set(gobject_ptr, property_name.as_ptr(), flags, std::ptr::null::<i32>());
      }

      Ok(())
   }

   fn get_playflag_state(&self, flag: u32) -> Result<bool> {
      let gobject_ptr = to_g_obj_pointer(self.pipeline.clone())?;

      let property_name = CString::new("flags")?;
      let mut flags: u32 = 0;

      let res = unsafe {
         g_object_get(
            gobject_ptr,
            property_name.as_ptr(),
            &mut flags as *mut u32 as *mut _, std::ptr::null::<i32>(),
         );
         (flags & flag) != 0
      };

      Ok(res)
   }
}

#[allow(dead_code)]
fn probe_props(element: &Element) {
   let props = element.list_properties();
   println!("\nProps of {}---------------", element.name().as_str());
   for prop in props {
      if prop.flags().contains(ParamFlags::READABLE) {
         let value: Value = element.property(prop.name());
         println!("Property: {} - Type: {:?} = ({value:?})", prop.name(), prop.value_type())
      } else {
         println!("Property: {} - Type: {:?}, VALUE NOT READABLE", prop.name(), prop.value_type())
      }
   }

   println!("End-----------------------\n");
}

fn to_g_obj_pointer<T>(to_object: T) -> Result<*mut GObject>
where
    T: IsA<Object> + Cast,
{
   let object = to_object.dynamic_cast::<Object>().unwrap();
   let ptr: *mut GstObject = object.to_glib_none().0;
   let gobject_ptr: *mut GObject = ptr as *mut GObject;
   Ok(gobject_ptr)
}


struct FrameQueueInfo {
   queued: bool,
   start_state: State,
   in_progress: bool,
}