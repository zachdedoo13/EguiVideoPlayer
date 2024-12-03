use anyhow::Result;
use eframe::egui::TextureId;
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::{AddressMode, Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, FilterMode, ImageCopyBuffer, ImageCopyTexture, ImageDataLayout, Origin3d, SamplerDescriptor, Texture, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension};
use gstreamer_video::video_frame::Readable;
use gstreamer_video::{VideoFormat, VideoFrame, VideoFrameExt};
use crate::wgpu::pack::WgpuRenderPack;

fn aligned_bytes_per_row(width: u32) -> u32 {
   let bytes_per_pixel = 4; // For example, RGBA format
   let bytes_per_row = width * bytes_per_pixel;
   let aligned_bytes_per_row = eframe::wgpu::util::align_to(bytes_per_row as u64, 256) as u32;

   aligned_bytes_per_row
}

pub struct Inner {
   pub texture: Texture,
   pub view: TextureView,
   pub buffer: Buffer,
   pub texture_id: TextureId,
}
impl Inner {
   fn create(width: u32, height: u32, render_pack: &WgpuRenderPack) -> Result<Self> {
      // tex
      let size = Extent3d {
         width,
         height,
         depth_or_array_layers: 1,
      };

      let texture = render_pack.device.create_texture(&TextureDescriptor {
         label: Some("Render texture"),
         size,
         mip_level_count: 1,
         sample_count: 1,
         dimension: TextureDimension::D2,
         format: TextureFormat::Rgba8UnormSrgb,
         usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
         view_formats: &[],
      });

      let view = texture.create_view(&TextureViewDescriptor {
         label: Some("Tex view"),
         format: Some(texture.format()),
         dimension: Some(TextureViewDimension::D2),
         aspect: TextureAspect::All,
         base_mip_level: 0,
         mip_level_count: Some(1), // Ensure this is within the texture's mip level count
         base_array_layer: 0,
         array_layer_count: Some(1),
      });

      // sampler
      let sampler_desc = SamplerDescriptor {
         label: Some("Texture Sampler"),
         address_mode_u: AddressMode::ClampToEdge,
         address_mode_v: AddressMode::ClampToEdge,
         address_mode_w: AddressMode::ClampToEdge,
         mag_filter: FilterMode::Nearest,
         min_filter: FilterMode::Nearest,
         mipmap_filter: FilterMode::Nearest,
         ..Default::default()
      };

      // buffer
      let aligned_bytes_per_row = aligned_bytes_per_row(width);
      let buffer_size = (aligned_bytes_per_row * height) as u64;
      // let buffer_size = ((width * height) * 4u32) as u64;

      let buffer = render_pack.device.create_buffer(&BufferDescriptor {
         label: Some("TextureBuffer"),
         size: buffer_size,
         usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
         mapped_at_creation: false,
      });

      // tex_id
      let texture_id = render_pack.renderer.write().register_native_texture_with_sampler_options(
         &render_pack.device,
         &view,
         sampler_desc,
      );


      Ok(Self {
         texture,
         view,
         buffer,
         texture_id,
      })
   }

   fn update(&self, data: Vec<u8>, render_state: &WgpuRenderPack) -> Result<()> {
      let width = self.texture.width();
      let height = self.texture.height();
      let aligned_bytes_per_row = aligned_bytes_per_row(width);

      let aligned_data = if aligned_bytes_per_row != width * 4 {
         let mut prog = vec![0u8; (aligned_bytes_per_row * height) as usize];
         for row in 0..height {
            let src_start = (row * width * 4) as usize;
            let src_end = src_start + (width * 4) as usize;
            let dst_start = (row * aligned_bytes_per_row) as usize;
            prog[dst_start..dst_start + (width * 4) as usize].copy_from_slice(&data[src_start..src_end]);
         }
         prog
      } else {
         data
      };

      // write to buffer then text
      {
         render_state.queue.write_buffer(&self.buffer, 0, &aligned_data);

         let mut encoder = render_state.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Tex encoder"),
         });

         encoder.copy_buffer_to_texture(
            ImageCopyBuffer {
               buffer: &self.buffer,
               layout: ImageDataLayout {
                  offset: 0,
                  bytes_per_row: Some(aligned_bytes_per_row),
                  rows_per_image: Some(self.texture.height()),
               },
            },
            ImageCopyTexture {
               texture: &self.texture,
               mip_level: 0,
               origin: Origin3d::ZERO,
               aspect: TextureAspect::All,
            },
            self.texture.size(),
         );

         render_state.queue.submit(Some(encoder.finish()));
      }

      // direct write to tex
      {
         // render_state.queue.write_texture(
         //    ImageCopyTexture {
         //       texture: &self.texture,
         //       mip_level: 0,
         //       origin: Origin3d::ZERO,
         //       aspect: TextureAspect::All,
         //    },
         //    &data,
         //    ImageDataLayout {
         //       offset: 0,
         //       bytes_per_row: Some(self.texture.width() * 4),
         //       rows_per_image: Some(self.texture.height()),
         //    },
         //    self.texture.size(),
         // );
         //
         // render_state.queue.submit([]);
      }

      Ok(())
   }
}

pub struct WgpuEguiDisplayTexture {
   pub inner: Option<Inner>,
}

impl WgpuEguiDisplayTexture {
   pub fn empty() -> Self {
      Self {
         inner: None,
      }
   }

   /// updates or creates and update the current texture
   pub fn create_or_update(&mut self, render_pack: &WgpuRenderPack, frame: VideoFrame<Readable>) -> Result<()> {
      let format = frame.format();
      if !matches!(format, VideoFormat::Rgba) { panic!("Gstreamer player must use the format sRGBA"); };

      let (width, height) = (frame.width(), frame.height());
      let data = frame.plane_data(0)?.to_owned();


      match &mut self.inner {
         // not created yet
         None => {
            let new_inner = Inner::create(width, height, render_pack)?;
            new_inner.update(data, render_pack)?;
            self.inner = Some(new_inner);
         }
         Some(inner) => {
            match inner.texture.width() != width || inner.texture.height() != height {
               // wrong size
               true => {
                  let new_inner = Inner::create(width, height, render_pack)?;
                  new_inner.update(data, render_pack)?;
                  self.inner = Some(new_inner);
               }
               // normal update
               false => {
                  inner.update(data, render_pack)?;
               }
            }
         }
      }

      Ok(())
   }

   #[allow(dead_code)]
   pub fn clear(&mut self) {
      self.inner = None;
   }
}