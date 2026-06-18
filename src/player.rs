use anyhow::Result;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::io::Cursor;

pub struct LocalPlayer {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Sink,
}

impl LocalPlayer {
    pub fn new() -> Result<Self> {
        let (_stream, handle) = OutputStream::try_default()
            .map_err(|e| anyhow::anyhow!("audio device error: {e}"))?;
        let sink = Sink::try_new(&handle)
            .map_err(|e| anyhow::anyhow!("audio sink error: {e}"))?;
        Ok(LocalPlayer { _stream, handle, sink })
    }

    pub fn play_bytes(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.sink = Sink::try_new(&self.handle)
            .map_err(|e| anyhow::anyhow!("audio sink error: {e}"))?;
        let source = Decoder::new(Cursor::new(bytes))
            .map_err(|e| anyhow::anyhow!("decode error: {e}"))?;
        self.sink.append(source);
        self.sink.play();
        Ok(())
    }

    pub fn queue_bytes(&self, bytes: Vec<u8>) -> Result<()> {
        let source = Decoder::new(Cursor::new(bytes))
            .map_err(|e| anyhow::anyhow!("decode error: {e}"))?;
        self.sink.append(source);
        self.sink.play();
        Ok(())
    }

    pub fn toggle_pause(&self) {
        if self.sink.is_paused() {
            self.sink.play();
        } else {
            self.sink.pause();
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        self.sink = Sink::try_new(&self.handle)
            .map_err(|e| anyhow::anyhow!("audio sink error: {e}"))?;
        Ok(())
    }

    pub fn set_volume(&self, vol: f32) {
        self.sink.set_volume(vol.clamp(0.0, 1.0));
    }

    pub fn volume(&self) -> f32 {
        self.sink.volume()
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn empty(&self) -> bool {
        self.sink.empty()
    }
}
