//! Procedurally generated sound effects.
//!
//! Two reasons this exists instead of a folder of .wav files:
//!
//! 1. Legal hygiene. "I'll just grab a placeholder gunshot for now" is exactly
//!    how someone else's asset ends up committed to a public repo and forgotten.
//! 2. It removes the "I need to find audio before I can test feel" excuse.
//!
//! These are placeholders and they sound like placeholders. What matters at
//! this stage is that pulling the trigger makes a noise, because silence makes
//! a weapon feel broken no matter how good the numbers are.
//!
//! Note that this module registers no Godot class at all -- these are plain
//! Rust functions that happen to return an engine type. Not everything needs
//! to be a node.

use godot::classes::audio_stream_wav::Format;
use godot::classes::AudioStreamWav;
use godot::prelude::*;

const SAMPLE_RATE: u32 = 22050;

/// A short, dry crack. Noise burst for the transient, low sine for the body.
pub fn gunshot(seed: u64) -> Gd<AudioStreamWav> {
    let length = 0.20_f32;
    let count = (SAMPLE_RATE as f32 * length) as usize;
    let mut data = PackedByteArray::new();
    data.resize(count * 2);

    let mut rng = Noise::new(seed);

    for i in 0..count {
        let t = i as f32 / SAMPLE_RATE as f32;
        // Exponential decay: loud at t=0, effectively silent by the end.
        let crack = rng.next_bipolar() * (-t * 34.0).exp();
        let body = (std::f32::consts::TAU * 85.0 * t).sin() * (-t * 26.0).exp();
        let tail = rng.next_bipolar() * (-t * 9.0).exp() * 0.18;
        let value = (crack * 0.75 + body * 0.55 + tail).clamp(-1.0, 1.0);
        write_sample(&mut data, i, value, 32000.0);
    }

    wav(data)
}

/// A softer tick, for reload steps and UI.
pub fn click(pitch_hz: f32, length: f32) -> Gd<AudioStreamWav> {
    let count = (SAMPLE_RATE as f32 * length) as usize;
    let mut data = PackedByteArray::new();
    data.resize(count * 2);

    for i in 0..count {
        let t = i as f32 / SAMPLE_RATE as f32;
        let value = (std::f32::consts::TAU * pitch_hz * t).sin() * (-t * 45.0).exp();
        write_sample(&mut data, i, value.clamp(-1.0, 1.0), 24000.0);
    }

    wav(data)
}

/// A dull thud for bullet impacts.
pub fn impact(seed: u64) -> Gd<AudioStreamWav> {
    let length = 0.12_f32;
    let count = (SAMPLE_RATE as f32 * length) as usize;
    let mut data = PackedByteArray::new();
    data.resize(count * 2);

    let mut rng = Noise::new(seed);

    for i in 0..count {
        let t = i as f32 / SAMPLE_RATE as f32;
        let mut value = rng.next_bipolar() * (-t * 60.0).exp() * 0.6;
        value += (std::f32::consts::TAU * 140.0 * t).sin() * (-t * 40.0).exp() * 0.4;
        write_sample(&mut data, i, value.clamp(-1.0, 1.0), 22000.0);
    }

    wav(data)
}

/// Little-endian signed 16-bit, which is what `Format::FORMAT_16_BITS` means.
fn write_sample(data: &mut PackedByteArray, index: usize, value: f32, scale: f32) {
    let sample = (value * scale) as i16;
    let bytes = sample.to_le_bytes();
    data[index * 2] = bytes[0];
    data[index * 2 + 1] = bytes[1];
}

fn wav(data: PackedByteArray) -> Gd<AudioStreamWav> {
    let mut stream = AudioStreamWav::new_gd();
    stream.set_format(Format::FORMAT_16_BITS);
    stream.set_mix_rate(SAMPLE_RATE as i32);
    stream.set_stereo(false);
    stream.set_data(&data);
    stream
}

/// A tiny deterministic noise source.
///
/// Godot's `RandomNumberGenerator` would work, but a seeded generator we own
/// means the exact same gunshot every run on every machine -- which matters
/// once anything is compared against a recorded expectation in a test.
struct Noise(u64);

impl Noise {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// xorshift64*: three shifts and a multiply. Not cryptographic, not trying
    /// to be; it is a noise generator for a placeholder gunshot.
    fn next_bipolar(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // Take the top 24 bits, map to 0..1, then to -1..1.
        ((bits >> 40) as f32 / 16_777_216.0) * 2.0 - 1.0
    }
}
