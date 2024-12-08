use std::sync::Arc;
use anyhow::Result;
use glam::Vec3;
use gstreamer_video::VideoFrameExt;
use terminal_framebuffer::full_color::FullColorFramebuffer;
use terminal_framebuffer::helper_functions::{enable_raw_mode, enable_wraparound, enter_alternate_screen, hide_cursor, index_to_uv, leave_alternate_screen, run_on_ctl_c, show_cursor};
use terminal_framebuffer::new_framework::{InternalNewFramebufferFramework, TerminalFramebuffer};
use vid_v2::gstreamer_internals::backend_framework::GstreamerBackendFramework;
use vid_v2::gstreamer_internals::backend_v2::BackendV2;
use rayon::prelude::*;

fn main() -> Result<()> {
   let uri = &*vid_v2::URI_ONLINE_CAR;

   let mut backend = BackendV2::init(uri)?;
   backend.start()?;

   hide_cursor()?;
   enter_alternate_screen()?;
   enable_wraparound()?;
   enable_raw_mode()?;

   let mut framebuffer = FullColorFramebuffer::new(Vec3::ZERO)?;

   let mut latest_frame = None;

   std::panic::set_hook(Box::new(|p| {
      leave_alternate_screen().unwrap();
      println!("Panicked {p:?}");
   }));

   loop {
      run_on_ctl_c(|| {
         terminal_framebuffer::helper_functions::disable_raw_mode().unwrap();
         show_cursor().unwrap();
         leave_alternate_screen().unwrap();
         std::process::exit(0);
      })?;

      if let Ok(update) = backend.update() {
         latest_frame = Some(update.frame);
      }

      framebuffer.update_size()?;

      if let Some(frame) = latest_frame.take() {
         let fbo_size = framebuffer.size();
         let aspect = framebuffer.aspect();
         let raw_data = framebuffer.get_data_vec_mut();

         let (width, height) = (frame.width(), frame.height());
         let frame_data = frame.plane_data(0).to_owned()?.to_vec();

         let frame_cont = Arc::new(frame_data);

         raw_data.par_iter_mut().enumerate().for_each({
            let clone = Arc::clone(&frame_cont);
            move |(i, data)| {
               let pix = sample_and_run(
                  i,
                  (width, height),
                  fbo_size,
                  aspect,
                  &*clone
               );
               *data = pix;
            }
         })

      } else {
         framebuffer.uv_fragment_par(|(_, last)| {
            *last
         });
      }


      framebuffer.draw_wraparound()?;
   }
}

fn sample_and_run(
   i: usize,
   size: (u32, u32),
   fbo_size: (u16, u16),
   _aspect: f32,
   data: &Vec<u8>
) -> Vec3 {
   let (width, height) = size;

   let uv = index_to_uv(i, fbo_size, 1.0) * 0.5 + 0.5;

   let nearest_x = (width as f32 * uv.x).floor() as u32;
   let nearest_y = (height as f32 * uv.y).floor() as u32;

   let nearest_index = ((nearest_y * width + nearest_x) * 4) as usize;
   let r = data[nearest_index];
   let g = data[nearest_index + 1];
   let b = data[nearest_index + 2];

   let col = Vec3::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
   col
}