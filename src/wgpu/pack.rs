use std::sync::Arc;
use anyhow::Context;
use eframe::egui_wgpu::RenderState;
use eframe::{Frame};
use eframe::egui::mutex::RwLock;
use eframe::wgpu::{Device, Queue};

pub struct WgpuRenderPack {
   pub device: Arc<Device>,
   pub queue: Arc<Queue>,
   pub renderer: Arc<RwLock<eframe::egui_wgpu::Renderer>>,
}
impl WgpuRenderPack {
   pub fn from_eframe_renderstate(render_state: &RenderState) -> Self {
      Self {
         device: Arc::clone(&render_state.device),
         queue: Arc::clone(&render_state.queue),
         renderer: Arc::clone(&render_state.renderer),
      }
   }
}

impl From<&RenderState> for WgpuRenderPack {
   fn from(render_state: &RenderState) -> Self {
      WgpuRenderPack::from_eframe_renderstate(render_state)
   }
}

impl From<RenderState> for WgpuRenderPack {
   fn from(render_state: RenderState) -> Self {
      WgpuRenderPack::from_eframe_renderstate(&render_state)
   }
}

impl TryFrom<&Frame> for WgpuRenderPack {
   type Error = anyhow::Error;

   fn try_from(frame: &Frame) -> Result<Self, Self::Error> {
     Ok(WgpuRenderPack::from_eframe_renderstate(frame.wgpu_render_state().context("Wgpu Not Enabled")?))
   }
}