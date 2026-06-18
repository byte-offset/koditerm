use anyhow::Result;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::io::Cursor;

pub struct LocalPlayer {
    _sink: MixerDeviceSink,
    player: Player,
}

#[allow(deprecated)]
pub fn list_devices() -> Vec<String> {
    let host = rodio::cpal::default_host();
    host.output_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

impl LocalPlayer {
    #[allow(deprecated)]
    pub fn new(device_name: Option<&str>) -> Result<Self> {
        let mut sink = match device_name {
            None => DeviceSinkBuilder::open_default_sink()
                .map_err(|e| anyhow::anyhow!("audio device error: {e}"))?,
            Some(name) => {
                let host = rodio::cpal::default_host();
                let device = host
                    .output_devices()
                    .map_err(|e| anyhow::anyhow!("cannot list devices: {e}"))?
                    .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
                    .ok_or_else(|| anyhow::anyhow!("audio device '{name}' not found"))?;
                DeviceSinkBuilder::from_device(device)
                    .map_err(|e| anyhow::anyhow!("device open error: {e}"))?
                    .open_stream()
                    .map_err(|e| anyhow::anyhow!("stream open error: {e}"))?
            }
        };
        sink.log_on_drop(false);
        let player = Player::connect_new(&sink.mixer());
        Ok(LocalPlayer { _sink: sink, player })
    }

    pub fn play_bytes(&mut self, bytes: Vec<u8>) -> Result<()> {
        // Non-blocking stop; append() will wait (~5ms) for the audio thread to drain.
        self.player.stop();
        self.player.play();
        let source = Decoder::new(Cursor::new(bytes))
            .map_err(|e| anyhow::anyhow!("decode error: {e}"))?;
        self.player.append(source);
        Ok(())
    }

    pub fn queue_bytes(&self, bytes: Vec<u8>) -> Result<()> {
        let source = Decoder::new(Cursor::new(bytes))
            .map_err(|e| anyhow::anyhow!("decode error: {e}"))?;
        self.player.append(source);
        Ok(())
    }

    pub fn toggle_pause(&self) {
        if self.player.is_paused() {
            self.player.play();
        } else {
            self.player.pause();
        }
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.player.play();
    }

    pub fn set_volume(&self, vol: f32) {
        self.player.set_volume(vol.clamp(0.0, 1.0));
    }

    pub fn volume(&self) -> f32 {
        self.player.volume()
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    pub fn empty(&self) -> bool {
        self.player.empty()
    }

    pub fn position(&self) -> u32 {
        self.player.get_pos().as_secs() as u32
    }
}
