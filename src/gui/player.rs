use crate::wgpu::display_texture::WgpuEguiDisplayTexture;
use crate::wgpu::pack::WgpuRenderPack;
use anyhow::Result;
use eframe::egui;
use eframe::egui::panel::TopBottomSide;
use eframe::egui::{CentralPanel, Frame, ImageSource, Key, Rect, Response, Sense, Slider, TopBottomPanel, Ui, UiBuilder, ViewportCommand};
use eframe::egui::load::SizedTexture;
use gstreamer::{ClockTime};
use lazy_bastard::lazy_bastard;
use crate::gstreamer_internals::backend_framework::{GstreamerBackendFramework, PlayFlags};

lazy_bastard!(
   pub struct SavedSettings {
      volume: f32 => 0.5,
      scroll_speed_mult: f32 => 5.0,
   }
);

lazy_bastard!(
   pub struct TempSettings {
      is_fullscreen: bool => false,
      queued_fullscreen_state: bool => false,
   }
);

pub struct VidioPlayer<B: GstreamerBackendFramework> {
   pub backend: Option<B>,
   display_texture: WgpuEguiDisplayTexture,
   saved_settings: SavedSettings,
   temp_settings: TempSettings,
}

/////////////////////
//// CONSTRUCTORS ///
/////////////////////
impl<Backend: GstreamerBackendFramework> VidioPlayer<Backend> {
   pub fn new(saved_settings: SavedSettings) -> Self {
      let backend = Backend::init(&*crate::URI_PATH_FRIEREN).unwrap();

      Self {
         backend: Some(backend),
         display_texture: WgpuEguiDisplayTexture::empty(),
         saved_settings,
         temp_settings: TempSettings::default(),
      }
   }

   pub fn new_with_uri(uri: &str, saved_settings: SavedSettings) -> Result<Self> {
      let mut player = VidioPlayer::new(saved_settings);
      player.open_uri(uri)?;
      Ok(player)
   }

   pub fn open_uri(&mut self, uri: &str) -> Result<()> {
      self.backend = Some(Backend::init(uri)?);
      Ok(())
   }

   pub fn close_current_player(&mut self) {
      self.backend = None;
   }
}


///////////////////////
//// Public METHODS ///
///////////////////////
impl<Backend: GstreamerBackendFramework> VidioPlayer<Backend> {
   pub fn show<R: Into<WgpuRenderPack>>(
      &mut self,
      ui: &mut Ui,
      in_pack: R,
   ) -> Result<()> {
      if self.backend.is_some() {
         let wgpu_render_pack: WgpuRenderPack = in_pack.into();
         self.update_frame(&wgpu_render_pack)?;
         self.show_internal(ui);
      } else {
         ui.label("Open a vidio to do shit");
      }

      ui.ctx().request_repaint();
      Ok(())
   }

   pub fn set_fullscreen(&mut self, to: bool) {
      if self.temp_settings.is_fullscreen != to {
         self.temp_settings.queued_fullscreen_state = to;
      }
   }
}


//////////////////////////////////
//// INTERNAL DISPLAY METHODS ////
//////////////////////////////////
impl<Backend: GstreamerBackendFramework> VidioPlayer<Backend> {
   #[inline(always)]
   fn get_backend(&mut self) -> &Backend {
      self.backend.as_ref().unwrap()
   }

   fn mut_backend(&mut self) -> &mut Backend {
      self.backend.as_mut().unwrap()
   }

   fn update_frame(&mut self, wgpu_render_pack: &WgpuRenderPack) -> Result<()> {
      if let Ok(update) = self.backend.as_mut().unwrap().update() {
         self.display_texture.create_or_update(wgpu_render_pack, update.frame)?;
      }
      Ok(())
   }

   fn manage_fullscreen_state(&mut self, ui: &mut Ui) {
      let temp = &mut self.temp_settings;
      if temp.queued_fullscreen_state != temp.is_fullscreen {
         match temp.queued_fullscreen_state {
            true => {
               temp.is_fullscreen = true;
               ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(true));
            }
            false => {
               temp.is_fullscreen = false;
               ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(false));
            }
         }
      }
   }

   fn show_internal(&mut self, ui: &mut Ui) {
      self.manage_fullscreen_state(ui);

      match self.temp_settings.is_fullscreen {
         true => {
            self.player_ui(ui, ui.available_rect_before_wrap());
         }
         false => {
            self.top_ui(ui);
            self.bottom_ui(ui);
            self.player_ui(ui, ui.ctx().screen_rect());
         }
      }
   }

   /// ### Panics
   fn menubar_inner(&mut self, ui: &mut Ui) {
      ui.menu_button("file", |ui| {
         if ui.button("Open file").clicked() {
            todo!()
         };

         if ui.button("Open url").clicked() {
            todo!()
         };
      });

      ui.menu_button("playback", |ui| {
         ui.menu_button("speed", |ui| {
            ui.horizontal(|ui| {
               if ui.button("25% ").clicked() { self.mut_backend().change_playback_speed(0.25).unwrap(); }
               if ui.button("50% ").clicked() { self.mut_backend().change_playback_speed(0.50).unwrap(); }
               if ui.button("75% ").clicked() { self.mut_backend().change_playback_speed(0.75).unwrap(); }
               if ui.button("100%").clicked() { self.mut_backend().change_playback_speed(1.00).unwrap(); }
            });

            ui.horizontal(|ui| {
               if ui.button("125%").clicked() { self.mut_backend().change_playback_speed(1.25).unwrap(); }
               if ui.button("150%").clicked() { self.mut_backend().change_playback_speed(1.50).unwrap(); }
               if ui.button("175%").clicked() { self.mut_backend().change_playback_speed(1.75).unwrap(); }
               if ui.button("200%").clicked() { self.mut_backend().change_playback_speed(2.00).unwrap(); }
            });




            let mut pbs = self.get_backend().current_playback_speed();
            if ui.add(Slider::new(&mut pbs, 0.1..=5.0)).drag_stopped() {
               self.mut_backend().change_playback_speed(pbs).unwrap();
            }
         });
      });

      ui.menu_button("video", |ui| {
         ui.menu_button("video track", |ui| {
            let probe = self.get_backend().get_probe().unwrap().clone();

            let current = self.get_backend().get_video_track().unwrap();
            for (i, (name_op, _id)) in probe.video_streams.iter().enumerate() {

               let title = match &name_op.name {
                  None => "No name".to_string(),
                  Some(name) => name.clone(),
               };

               let formated_title = match i as u32 == current {
                  true => format!("{i} | {title} #"),
                  false => format!("{i} | {title}"),
               };

               if ui.button(formated_title).clicked() {
                  self.mut_backend().set_video_track(i as u32).unwrap()
               }
            }
         });
      });

      ui.menu_button("audio", |ui| {
         ui.menu_button("audio track", |ui| {
            let probe = self.get_backend().get_probe().unwrap().clone();

            let current = self.get_backend().get_audio_track().unwrap();
            for (i, (name_op, _id)) in probe.audio_streams.iter().enumerate() {

               let title = match &name_op.name {
                  None => "No name".to_string(),
                  Some(name) => name.clone(),
               };

               let formated_title = match i as u32 == current {
                  true => format!("{i} | {title} #"),
                  false => format!("{i} | {title}"),
               };

               if ui.button(formated_title).clicked() {
                  self.mut_backend().set_audio_track(i as u32).unwrap()
               }
            }
         });

         ui.menu_button("Audio devices", |ui| {
            let current_device = self.get_backend().get_current_audio_device();
            for (name, id) in self.get_backend().list_audio_devices().unwrap() {
               let mut is_hash = false;
               if let Some(device) = &current_device {
                  if device.as_str() == id {
                     is_hash = true;
                  }
               }

               if ui.button(format!("{name}{}", if is_hash {" #"} else {""})).clicked() {
                  self.mut_backend().set_audio_device(id.as_str()).unwrap();
               }
            }
         });

         ui.menu_button("Mode", |ui| {
            ui.label("Put surround sound settings or something hear");
         });

         ui.menu_button("Vol scroll speed", |ui| {
            ui.add(Slider::new(&mut self.saved_settings.scroll_speed_mult, 1.0..=20.0));
         });

         let mut val = self.get_backend().get_current_volume();
         if ui.add(Slider::new(&mut val, self.get_backend().get_volume_range())).hovered() {
            self.mut_backend().set_volume(val).unwrap();
         }
      });

      ui.menu_button("subtitles", |ui| {

         ui.menu_button("subtitle track", |ui| {
            let probe = self.get_backend().get_probe().unwrap().clone();

            let current = self.get_backend().get_sub_track().unwrap();
            for (i, (name_op, _id)) in probe.captions.iter().enumerate() {


               let title = match &name_op {
                  None => "No name".to_string(),
                  Some(name) => name.clone(),
               };

               let formated_title = match i as u32 == current {
                  true => format!("{i} | {title} #"),
                  false => format!("{i} | {title}"),
               };

               if ui.button(formated_title).clicked() {
                  self.mut_backend().set_sub_track(i as u32).unwrap()
               }
            }
         });

         let mut bool = self.get_backend().get_playflag_state(PlayFlags::SUBTITLES).unwrap();
         if ui.checkbox(&mut bool, "enabled").changed() {
            self.mut_backend().toggle_playflag(bool, PlayFlags::SUBTITLES).unwrap();
         };
      });

      ui.menu_button("tools", |ui| {
         if ui.button("open settings").clicked() {
            todo!()
         }

         if ui.button("Fullscreen").clicked() {
            self.temp_settings.queued_fullscreen_state = !self.temp_settings.queued_fullscreen_state;
         }

         if ui.button("Fullscreen").clicked() {
            self.temp_settings.queued_fullscreen_state = !self.temp_settings.queued_fullscreen_state;
         }

         if ui.button("Step_one_frame").clicked() {
            self.mut_backend().seek_frames(1).unwrap();
            // self.mut_backend().queue_frame_update();
         }

         if ui.button("Step_min_one_frame").clicked() {
            self.mut_backend().seek_frames(-1).unwrap();
            // self.mut_backend().queue_frame_update();
         }

         if ui.button("Step_100_frame").clicked() {
            self.mut_backend().seek_frames(100).unwrap();
            // self.mut_backend().queue_frame_update();
         }

         if ui.button("Step_back_100_frame").clicked() {
            self.mut_backend().seek_frames(-100).unwrap();
            // self.mut_backend().queue_frame_update();
         }

         if ui.button("Reverse").clicked() {
            // self.mut_backend().seek_frames(1).unwrap();
            // self.mut_backend().queue_frame_update();

            self.mut_backend().change_playback_speed(10.0).unwrap();
         }

         let mut pbs = self.get_backend().current_playback_speed();
         if ui.add(Slider::new(&mut pbs, 0.1..=5.0)).drag_stopped() {
            self.mut_backend().change_playback_speed(pbs).unwrap();
         }
      });
   }

   /// TODO funky
   fn menubar(&mut self, ui: &mut Ui, vertical: bool) {
      egui::menu::bar(ui, |ui| {
         let mut cont = |ui: &mut Ui| {
            match self.get_backend().get_probe() {
               Err(err) => { ui.label(format!("{err}")); },
               Ok(_) => {
                  // match probe_res {
                  //    Ok(_) => {
                  //       self.menubar_inner(ui);
                  //    }
                  //    Err(err) => {
                  //       ui.label(format!("Probe error => {err}"));
                  //    }
                  // };

                  self.menubar_inner(ui);
               }
            }
         };

         if vertical {
            ui.vertical(|ui| {
               cont(ui);
            });
         } else {
            cont(ui);
         }
      });
   }

   fn player_interaction(&mut self, ui: &mut Ui, resp: Response) {
      if resp.double_clicked() {
         self.temp_settings.queued_fullscreen_state = !self.temp_settings.queued_fullscreen_state;
      }

      // keyboard input
      ui.ctx().input(|i| {
         if i.key_pressed(Key::Space) {
            match self.get_backend().is_paused() {
               true => {
                  self.mut_backend().start().unwrap();
               }
               false => {
                  self.mut_backend().stop().unwrap();
               }
            }
         }
      });

      if resp.hovered() {
         ui.ctx().input(|i| {
            let raw_spd = i.raw_scroll_delta.y;
            let unit = raw_spd / 40.0;

            if unit != 0.0 {
               let digit = (unit * 0.01) * self.saved_settings.scroll_speed_mult * 2.5;
               let c = self.get_backend().get_current_volume();
               let set = (c + digit as f64).clamp(0.0, *self.get_backend().get_volume_range().end());
               self.mut_backend().set_volume(set).unwrap();
            }
         });
      }

      resp.context_menu(|ui| {
         ui.set_max_width(75.0);
         self.menubar(ui, true);
      });
   }

   fn player_ui(&mut self, ui: &mut Ui, major_rect: Rect) {
      CentralPanel::default().frame(Frame::none()).show_inside(ui, |ui| {
         let resp_rect = ui.available_rect_before_wrap();
         if let Some(inner) = &self.display_texture.inner {
            let correct_size = inner.texture.size();
            let aspect = correct_size.width as f32 / correct_size.height as f32;

            let max_width = major_rect.width();
            let max_height = major_rect.height();
            let mut inner_width = max_width;
            let mut inner_height = inner_width / aspect;

            if inner_height > max_height {
               inner_height = max_height;
               inner_width = inner_height * aspect;
            }

            let mut inner_rect = major_rect;
            inner_rect.set_width(inner_width);
            inner_rect.set_height(inner_height);
            inner_rect.set_center(major_rect.center());

            ui.allocate_new_ui(UiBuilder::new().max_rect(inner_rect), |ui| {
               ui.image(ImageSource::Texture(SizedTexture::new(inner.texture_id, ui.available_size())));
            });
         };

         let resp = ui.allocate_rect(resp_rect, Sense {
            click: true,
            drag: true,
            focusable: false,
         });
         self.player_interaction(ui, resp);
      });
   }

   fn top_ui(&mut self, ui: &mut Ui) {
      TopBottomPanel::new(TopBottomSide::Top, "top").show_inside(ui, |ui| {
         self.menubar(ui, false);
      });
   }

   fn bottom_ui(&mut self, ui: &mut Ui) {
      TopBottomPanel::new(TopBottomSide::Bottom, "bottom").show_inside(ui, |ui| {
         ui.horizontal(|ui| {
            if ui.button("SwitchFullscreenState").clicked() {
               self.set_fullscreen(!self.temp_settings.is_fullscreen);
            };

            if ui.button("Play").clicked() {
               self.mut_backend().start().unwrap();
            }

            if ui.button("Pause").clicked() {
               self.mut_backend().stop().unwrap();
            }

            let mut change = self.get_backend().timecode().seconds_f64();
            let max = self.get_backend().get_duration().unwrap().seconds_f64() - self.get_backend().get_frametime();
            if ui.add(Slider::new(&mut change, 0.0..=max).prefix("Keyframe ")).changed() {
               self.mut_backend().seek_timeline(
                  ClockTime::from_seconds_f64(change),
                  true
               ).unwrap();

               self.mut_backend().queue_frame_update();
            }

            // let mut change = self.get_backend().timecode().seconds_f64();
            // let max = self.get_backend().get_duration().unwrap().seconds_f64() - self.get_backend().get_frametime();
            // if ui.add(Slider::new(&mut change, 0.0..=max).prefix("Exact ")).changed() {
            //    self.mut_backend().seek_timeline(
            //       ClockTime::from_seconds_f64(change),
            //       true
            //    ).unwrap();
            //
            //    self.mut_backend().queue_frame_update();
            // }
         })
      });
   }
}