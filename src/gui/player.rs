use crate::gstreamer_internals::player_backend::GstreamerBackend;
use crate::gstreamer_internals::prober::{PollRes, Probe};
use crate::wgpu::display_texture::WgpuEguiDisplayTexture;
use crate::wgpu::pack::WgpuRenderPack;
use anyhow::Result;
use eframe::egui;
use eframe::egui::panel::TopBottomSide;
use eframe::egui::{CentralPanel, Frame, Rect, TopBottomPanel, Ui, ViewportCommand};
use lazy_bastard::lazy_bastard;

lazy_bastard!(
   pub struct SavedSettings {
      volume: f32 => 0.5,
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

/// Constructors
impl VidioPlayer {
   pub fn new(saved_settings: SavedSettings, setup_settings: SetupSettings) -> Self {
      Self {
         backend: Some(GstreamerBackend::init(&*crate::URI_PATH_FRIEREN).unwrap()), // TODO debug only
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

/// Methods
impl VidioPlayer {
   pub fn show<R: Into<WgpuRenderPack>>(&mut self, ui: &mut Ui, in_pack: R) -> Result<()> {
      // update
      match self.backend.is_some() {
         true => {
            let wgpu_render_pack: WgpuRenderPack = in_pack.into();
            self.update_frame(&wgpu_render_pack)?;

            // render
            self.show_internal(ui);
         }
         false => {
            ui.label("Open a vidio to do shit");

            // display open vid image
         }
      }

      if ui.button("SwitchFullscreenState").clicked() {
         self.set_fullscreen(!self.temp_settings.is_fullscreen);
      }

      Ok(())
   }

   pub fn set_fullscreen(&mut self, to: bool) {
      if self.temp_settings.is_fullscreen != to {
         self.temp_settings.queued_fullscreen_state = to;
      }
   }
}

/// Internal Display Methods
impl VidioPlayer {
   /// Panics if backend is none when called
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
            self.player_ui(ui, ui.ctx().screen_rect());
         }
         false => {
            self.player_ui(ui, ui.available_rect_before_wrap());
         }
      }
   }

   fn menubar(&mut self, ui: &mut Ui, vertical: bool) {
      egui::menu::bar(ui, |ui| {
         let mut cont = |ui: &mut Ui| {
            match self.backend.as_mut().unwrap().probe.check() {
               PollRes::NotInitialized => {}
               PollRes::InProgress => {}
               PollRes::Available(p) | PollRes::JustBecameAvailable(p) => {
                  if let Ok(p) = p {
                     self.menu_inner(ui, p); // todo broken
                  }
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

   fn menu_inner(&mut self, ui: &mut Ui, probe: &Probe) {
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
            // let current = self.raw.gstreamer_player.current_video_track().unwrap() as usize;
            // for (track, index) in &self.raw.gstreamer_player.probe_result.video_streams {
            //    let name = track.name.clone().unwrap_or("Unnamed_track".to_string());
            //    if ui.button(format!("{name} |{index}| {}", if *index == current { "#" } else { "" })).clicked() {
            //       self.raw.gstreamer_player.video_track(*index as u32).unwrap()
            //    }
            // }
         });
         if ui.button("screen shot").clicked() {
            todo!()
         }
      });

      ui.menu_button("audio", |ui| {
         ui.menu_button("video_track", |ui| {
            // let current = self.raw.gstreamer_player.current_audio_track().unwrap() as usize;
            // for (track, index) in &self.raw.gstreamer_player.probe_result.audio_streams {
            //    let name = track.name.clone().unwrap_or("Unnamed_track".to_string());
            //    if ui.button(format!("{name} |{index}| {}", if *index == current { "#" } else { "" })).clicked() {
            //       self.raw.gstreamer_player.audio_track(*index as u32).unwrap()
            //    }
            // }
         });

         ui.menu_button("Audio devices", |_ui| {
            // todo!()
         });

         ui.menu_button("Mode", |_ui| {
            // todo!()
         });

         ui.menu_button("Vol scroll speed", |ui| {
            // ui.add(Slider::new(&mut self.saved.scroll_speed_mult, 1.0..=20.0));
            todo!()
         });

         // let v = self.saved.volume * 100.0;
         // ui.label(format!("Current volume {v:.2}%{}", if v == 100.0 { "" } else { "." }));
      });

      ui.menu_button("subtitles", |ui| {
         ui.menu_button("subtitle track", |ui| {
            // let current = self.raw.gstreamer_player.current_sub_track().unwrap() as usize;
            // for (sub, index) in &self.raw.gstreamer_player.probe_result.captions {
            //    let name = sub.clone().unwrap_or("Unnamed_track".to_string());
            //    if ui.button(format!("{name} |{index}| {}", if *index == current { "#" } else { "" })).clicked() {
            //       self.raw.gstreamer_player.sub_track(*index as u32).unwrap()
            //    }
            // }
         });

         ui.menu_button("Audio devices", |_ui| {
            // todo!()
         });

         ui.menu_button("Mode", |_ui| {
            // todo!()
         });
      });

      ui.menu_button("tools", |ui| {
         if ui.button("open settings").clicked() {
            todo!()
         }
      });
   }

   fn player_ui(&mut self, ui: &mut Ui, rect: Rect) {
      // menubar
      TopBottomPanel::new(TopBottomSide::Top, "top").show_inside(ui, |ui| {
         self.menubar(ui, false);
      });


      // Timeline and other buttons
      TopBottomPanel::new(TopBottomSide::Bottom, "bottom").show_inside(ui, |ui| {});

      // Main video player
      CentralPanel::default().frame(Frame::none()).show_inside(ui, |ui| {});
   }
}





