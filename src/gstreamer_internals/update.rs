use anyhow::Context;
use gstreamer::{ClockTime, Sample};
use gstreamer_video::video_frame::Readable;
use gstreamer_video::{VideoFrame, VideoInfo};

pub struct FrameUpdate {
   pub frame: VideoFrame<Readable>,
   pub timecode: ClockTime,
}

impl FrameUpdate {
   pub fn from_sample(sample: Sample) -> anyhow::Result<(Self, VideoInfo)> {
      let buffer = sample.buffer_owned().context("No buffer")?;
      let caps = sample.caps().context("No caps")?;
      let vidio_info = VideoInfo::from_caps(&caps)?;

      let timecode = buffer.pts().context("No timecode in video frame")?;

      let frame = VideoFrame::from_buffer_readable(buffer, &vidio_info)
          .ok().context("Failed to grab frame")?;

      Ok((
         Self { frame, timecode},
         vidio_info,
      ))
   }
}