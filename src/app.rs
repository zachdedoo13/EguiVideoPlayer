use eframe::{App, Renderer};
use eframe::egui::{CentralPanel, Context, Frame};
use vid_v2::gstreamer_internals::backend_v2::BackendV2;
use vid_v2::gui::player::{SavedSettings, VidioPlayer};

fn main() {
   vid_v2::sleep_directives::prevent_sleep();

   let native_options = eframe::NativeOptions {
      renderer: Renderer::Wgpu,
      ..Default::default()
   };
   eframe::run_native(
      "Video player",
      native_options, Box::new(|_| Ok(Box::new(TestApp {
         player: VidioPlayer::new(SavedSettings::default()),
      }))),
   ).unwrap();
}

pub struct TestApp {
   player: VidioPlayer<BackendV2>,
}
impl App for TestApp {
   fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
      CentralPanel::default()
          .frame(Frame::none())
          .show(ctx, |ui| {
             self.player.show(ui, frame.wgpu_render_state().unwrap()).unwrap();
          });
   }
}