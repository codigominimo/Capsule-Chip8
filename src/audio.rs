//! Pitido simple ligado al sound timer del Chip8 (440 Hz).

use rodio::source::{SineWave, Source};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};
use std::time::Duration;

pub struct Beeper {
    // El sink del dispositivo debe vivir mientras el player exista.
    _sink: MixerDeviceSink,
    player: Player,
    playing: bool,
    frequency: f32,
    volume: f32,
}

impl Beeper {
    pub fn new() -> Result<Self, String> {
        let sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        let player = Player::connect_new(sink.mixer());
        Ok(Self {
            _sink: sink,
            player,
            playing: false,
            frequency: 440.0,
            volume: 0.12,
        })
    }

    /// Enciende o apaga el tono. `SineWave` es una fuente infinita, así que
    /// basta con append una vez y limpiar el player cuando el timer llega a cero.
    pub fn set(&mut self, active: bool) {
        if active == self.playing {
            return;
        }
        self.playing = active;
        if active {
            let tone = SineWave::new(self.frequency)
                .amplify(self.volume)
                .fade_in(Duration::from_millis(4));
            self.player.append(tone);
        } else {
            self.player.clear();
        }
    }
}
