use std::sync::{Arc, RwLock};
use crate::gstreamer_internals::player_backend::GstreamerBackend;
use crate::wgpu::display_texture::WgpuEguiDisplayTexture;
use crate::wgpu::pack::WgpuRenderPack;
use anyhow::Result;
use eframe::egui;
use eframe::egui::panel::TopBottomSide;
use eframe::egui::{CentralPanel, Frame, ImageSource, Key, PointerButton, Rect, Response, Sense, Slider, TopBottomPanel, Ui, UiBuilder, ViewportCommand};
use eframe::egui::load::SizedTexture;
use egui_logger::EguiLogger;
use gstreamer::ClockTime;
use lazy_bastard::lazy_bastard;
use log::{debug, Level, Log, Record};
use crate::gstreamer_internals::prober::Probe;

lazy_bastard!(
   pub struct SavedSettings {
      volume: f32 => 0.5,
      scroll_speed_mult: f32 => 0.02,
   }
);

lazy_bastard!(
   pub struct TempSettings {
      is_fullscreen: bool => false,
      queued_fullscreen_state: bool => false,
   }
);

lazy_bastard!(
   pub struct SetupSettings {
      allow_user_to_open_other_media: bool => true,
   }
);

pub struct VidioPlayer {
   pub backend: Option<GstreamerBackend>,
   display_texture: WgpuEguiDisplayTexture,
   saved_settings: SavedSettings,
   temp_settings: TempSettings,
   setup_settings: SetupSettings,
}

/////////////////////
//// CONSTRUCTORS ///
/////////////////////
impl VidioPlayer {
   pub fn new(saved_settings: SavedSettings, setup_settings: SetupSettings) -> Self {
      let mut backend = GstreamerBackend::init(&*crate::URI_PATH_FRIEREN).unwrap();
      backend.force_update_now(true).unwrap();

      Self {
         backend: Some(backend),
         display_texture: WgpuEguiDisplayTexture::empty(),
         saved_settings,
         temp_settings: TempSettings::default(),
         setup_settings,
      }
   }

   pub fn new_with_uri(uri: &str, saved_settings: SavedSettings, setup_settings: SetupSettings) -> Result<Self> {
      let mut player = VidioPlayer::new(saved_settings, setup_settings);
      player.open_uri(uri)?;
      Ok(player)
   }

   pub fn open_uri(&mut self, uri: &str) -> Result<()> {
      self.backend = Some(GstreamerBackend::init(uri)?);
      Ok(())
   }

   pub fn close_current_player(&mut self) {
      self.backend = None;
   }
}


///////////////////////
//// Public METHODS ///
///////////////////////
impl VidioPlayer {
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
impl VidioPlayer {
   fn get_backend(&mut self) -> &GstreamerBackend {
      self.backend.as_ref().unwrap()
   }

   fn mut_backend(&mut self) -> &mut GstreamerBackend {
      self.backend.as_mut().unwrap()
   }

   fn update_frame(&mut self, wgpu_render_pack: &WgpuRenderPack) -> Result<()> {
      if let Ok(update) = self.backend.as_mut().unwrap().poll_update() {
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
      let probe = self.get_backend().probe.as_ref().unwrap().as_ref().unwrap();

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
            if ui.button("25%").clicked() { todo!() }
            if ui.button("50%").clicked() { todo!() }
            if ui.button("75%").clicked() { todo!() }
            if ui.button("100%").clicked() { todo!() }
         });
      });

      ui.menu_button("video", |ui| {
         ui.menu_button("video_track", |ui| {
            for (vid_track, vid_index) in probe.video_streams.iter() {
               ui.label(format!("Stream {vid_index} Named {:?}", vid_track.name));
            }
         });
         if ui.button("screen shot").clicked() {
            todo!()
         }
      });

      ui.menu_button("audio", |ui| {
         ui.menu_button("video_track", |ui| {
         });

         ui.menu_button("Audio devices", |_ui| {
         });

         ui.menu_button("Mode", |_ui| {
         });

         ui.menu_button("Vol scroll speed", |ui| {
            ui.add(Slider::new(&mut self.saved_settings.scroll_speed_mult, 1.0..=20.0));
         });
      });

      ui.menu_button("subtitles", |ui| {
         ui.menu_button("subtitle track", |ui| {
         });

         ui.menu_button("Audio devices", |_ui| {
         });

         ui.menu_button("Mode", |_ui| {
         });
      });

      ui.menu_button("tools", |ui| {
         if ui.button("open settings").clicked() {
            todo!()
         }
      });

      if ui.button("Fullscreen").clicked() {
         self.temp_settings.queued_fullscreen_state = !self.temp_settings.queued_fullscreen_state;
      }

      if ui.button("Test").clicked() {
         self.mut_backend().seek_normal(ClockTime::from_seconds_f64(120.0)).unwrap();
         self.mut_backend().queue_forced_update();
      }
   }

   /// TODO funky
   fn menubar(&mut self, ui: &mut Ui, vertical: bool) {
      egui::menu::bar(ui, |ui| {
         let mut cont = |ui: &mut Ui| {
            match &self.get_backend().probe {
               None => { ui.label("Waiting for probe to complete"); },
               Some(probe_res) => {
                  match probe_res {
                     Ok(_) => {
                        self.menubar_inner(ui);
                     }
                     Err(err) => {
                        ui.label(format!("Probe error => {err}"));
                     }
                  };
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
            match self.get_backend().is_paused {
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
               // let digit = (unit * 0.01) * self.saved.scroll_speed_mult;
               // if let Ok(c) = self.raw.gstreamer_player.get_volume() {
               //    let set = (c + digit as f64).clamp(0.0, 1.0);
               //    self.saved.volume = set as f32;
               //    self.raw.gstreamer_player.set_volume(set).unwrap();
               // }
               todo!()
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
         })
      });
   }
}