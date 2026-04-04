// Colored-noise generator. We construct a frequency-domain spectrum — one
// complex number per frequency bin, with the amplitude defining the noise
// "color" and a random phase making it sound like noise rather than a chord —
// then inverse-FFT to produce time-domain audio samples written as a WAV file.
//
// <https://en.wikipedia.org/wiki/Colors_of_noise>

use rand::RngExt;
use rustfft::{FftPlanner, num_complex::Complex};
use std::io::BufWriter;
use std::io::Write;
use zerocopy::{
    Immutable, IntoBytes,
    little_endian::{U16, U32},
};

#[derive(IntoBytes, Immutable)]
#[repr(u16)]
enum WaveFormatCategory {
    /// Microsoft Pulse Code Modulation (PCM) format
    Pcm = 0x0001u16.to_le(),
}

#[derive(IntoBytes, Immutable)]
#[repr(C, packed)]
struct FormatChunkCommon<FSF> {
    format_tag: WaveFormatCategory,
    channels: U16,
    samples_per_sec: U32,
    avg_bytes_per_sec: U32,
    block_align: U16,
    format_specific: FSF,
}

#[derive(IntoBytes, Immutable)]
#[repr(C, packed)]
struct FormatChunkPcm {
    bits_per_sample: U16,
}

const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
// Upper bound of human hearing, matching CD-quality audio (44.1 kHz).
const MAX_FREQUENCY: u32 = 22050;
// Must be at least 2× the highest frequency we want to reproduce (Nyquist).
// <https://en.wikipedia.org/wiki/Nyquist_frequency>
const SAMPLES_PER_SECOND: u32 = MAX_FREQUENCY * 2;
const AVG_BYTES_PER_SECOND: u32 =
    CHANNELS as u32 * SAMPLES_PER_SECOND * (BITS_PER_SAMPLE / 8) as u32;

enum Color {
    White,
    Pink,
    Brownian,
    Blue,
    Violet,
    Grey,
}

struct Args {
    color: Color,
    duration: u32,
}

/// IEC 61672-1 A-weighting relative response at a given frequency.
///
/// Returns `R_A(hz)` in linear scale (not dB). The result is approximately 1.0
/// at 1000 Hz by the standard's convention. Used both for grey noise spectral
/// shaping (inverse A-weighting) and for A-weighted loudness normalization.
///
/// <https://en.wikipedia.org/wiki/A-weighting>
fn r_a(hz: f64) -> f64 {
    ((12194.0f64).powi(2) * hz.powi(4))
        / ((hz.powi(2) + 20.6f64.powi(2))
            * f64::sqrt((hz.powi(2) + 107.7f64.powi(2)) * (hz.powi(2) + 737.9f64.powi(2)))
            * (hz.powi(2) + (12194.0f64).powi(2)))
}

fn parse_args() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;

    let mut color = None;
    let mut duration = 20;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Short('d') | Long("duration") => {
                duration = parser.value()?.parse()?;
            }
            Value(val) if color.is_none() => {
                color = Some(val.parse_with(|color| match color {
                    "white" => Ok(Color::White),
                    "pink" => Ok(Color::Pink),
                    "brownian" => Ok(Color::Brownian),
                    "blue" => Ok(Color::Blue),
                    "violet" => Ok(Color::Violet),
                    "grey" => Ok(Color::Grey),
                    _ => Err("unknown color"),
                })?);
            }
            Long("help") => {
                println!("Usage: trough [-d|--duration=SECONDS] COLOR");
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    let color = color.unwrap_or(Color::White);
    Ok(Args { color, duration })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let mut rng = rand::rng();

    // Target A-weighted RMS for the time-domain output. Each closure below
    // defines only the spectral *shape*, and `noise()` normalizes the spectrum
    // so that its A-weighted energy matches this target — making all colors
    // sound approximately equally loud regardless of spectral tilt.
    //
    // With avg_amplitude = 8 and 22049 positive-frequency bins, this matches
    // the historical white noise level (~5% of i16 full-scale). Colors with
    // energy concentrated where hearing is insensitive (brownian, grey) need
    // much higher physical RMS to match, and may approach i16 clipping at this
    // target. Reduce `avg_amplitude` if clipping is observed.
    let avg_amplitude = 8.;
    let num_positive_bins = (SAMPLES_PER_SECOND / 2 - 1) as f64;
    let target_rms = avg_amplitude * (2.0 * num_positive_bins).sqrt();

    // Each closure below defines a spectral shape: the amplitude of each
    // frequency bin. `noise()` handles the absolute scaling. The `from_polar`
    // pattern encodes amplitude and phase into a single complex FFT bin.
    // <https://en.wikipedia.org/wiki/Spectral_density>
    match args.color {
        Color::White => noise(args.duration, target_rms, |spectrum| {
            for bin in spectrum {
                *bin = Complex::from_polar(1., rng.random::<f64>() * std::f64::consts::TAU);
            }
        })?,
        // Amplitude ∝ 1/√f (power ∝ 1/f). skip(20): below 20 Hz is inaudible.
        // <https://en.wikipedia.org/wiki/Pink_noise>
        Color::Pink => noise(args.duration, target_rms, |spectrum| {
            for (hz, bin) in spectrum.iter_mut().enumerate().skip(20) {
                *bin = Complex::from_polar(
                    1. / ((hz + 1) as f64).sqrt(),
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        // Amplitude ∝ 1/f (power ∝ 1/f²). Steeper rolloff than pink.
        // <https://en.wikipedia.org/wiki/Brownian_noise>
        Color::Brownian => noise(args.duration, target_rms, |spectrum| {
            for (hz, bin) in spectrum.iter_mut().enumerate().skip(20) {
                *bin = Complex::from_polar(
                    1. / (hz + 1) as f64,
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        // Amplitude ∝ √f — the spectral inverse of pink.
        Color::Blue => noise(args.duration, target_rms, |spectrum| {
            for (hz, bin) in spectrum.iter_mut().enumerate() {
                *bin = Complex::from_polar(
                    ((hz + 1) as f64).sqrt(),
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        // Amplitude ∝ f — the spectral inverse of brownian.
        Color::Violet => noise(args.duration, target_rms, |spectrum| {
            for (hz, bin) in spectrum.iter_mut().enumerate() {
                *bin = Complex::from_polar(
                    (hz + 1) as f64,
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        // Inverts A-weighting (a model of human loudness perception across
        // frequencies) so the noise sounds perceptually flat. See `r_a`.
        // <https://en.wikipedia.org/wiki/A-weighting>
        Color::Grey => {
            let ra1000 = r_a(1000.);
            noise(args.duration, target_rms, |spectrum| {
                for (hz, bin) in spectrum.iter_mut().enumerate().skip(20) {
                    // hz is 0-indexed within the closure, but the closure receives
                    // bins starting at frequency 1, so actual frequency is hz + 1.
                    let a_weight = r_a((hz + 1) as f64) / ra1000;
                    *bin = Complex::from_polar(
                        1. / a_weight,
                        rng.random::<f64>() * std::f64::consts::TAU,
                    );
                }
            })?
        }
    }

    Ok(())
}

fn noise(
    duration_in_seconds: u32,
    target_rms: f64,
    mut spectrum_setup: impl FnMut(&mut [Complex<f64>]),
) -> Result<(), std::io::Error> {
    let sample_data_len = AVG_BYTES_PER_SECOND * duration_in_seconds;
    let format = FormatChunkCommon {
        format_tag: WaveFormatCategory::Pcm,
        channels: 1.into(),
        samples_per_sec: SAMPLES_PER_SECOND.into(),
        avg_bytes_per_sec: AVG_BYTES_PER_SECOND.into(),
        block_align: (CHANNELS * BITS_PER_SAMPLE / 8).into(),
        format_specific: FormatChunkPcm {
            bits_per_sample: BITS_PER_SAMPLE.into(),
        },
    };

    let out = std::fs::File::create("audio.wav")?;
    let mut out = BufWriter::new(out);
    out.write_all(b"RIFF")?;
    // 5 fixed-size u32 fields follow: WAVE, fmt chunk id, fmt chunk size,
    // data chunk id, and data chunk size.
    out.write_all(
        &(sample_data_len + 5 * 4 + std::mem::size_of_val(&format) as u32).to_le_bytes(),
    )?;
    out.write_all(b"WAVE")?;
    write_chunk(b"fmt ", format, &mut out)?;
    out.write_all(b"data")?;
    out.write_all(&sample_data_len.to_le_bytes())?;

    let length = SAMPLES_PER_SECOND as usize;
    let mut real_planner = FftPlanner::<f64>::new();
    let c2r = real_planner.plan_fft_inverse(length);

    let mut spectrum = [Complex::ZERO; SAMPLES_PER_SECOND as usize];
    let mut time = [Complex::ZERO; SAMPLES_PER_SECOND as usize];
    let mut scratch = Vec::new();
    scratch.resize(c2r.get_immutable_scratch_len(), Complex::ZERO);

    let mut rng = rand::rng();
    // Fade-in: starts at −1 (full cancellation: `amp + amp × −1 = 0`) and ramps
    // to 0 over ~10,000 samples (~0.23 s). Avoids a startup click from the
    // abrupt onset of audio at sample 0.
    let mut dampen = -1.0;
    // We generate one second of audio per iteration. Each second gets a slightly
    // perturbed spectrum so the noise evolves over time rather than being a
    // repeating 1-second loop.
    for interval in 0..duration_in_seconds {
        let (pos, neg) = spectrum.split_at_mut(SAMPLES_PER_SECOND as usize / 2);
        if interval == 0 {
            spectrum_setup(&mut pos[1..]);
            // DC (bin 0) would be an inaudible constant offset.
            pos[0] = Complex::ZERO;

            // Normalize so all noise colors are perceptually equally loud,
            // using A-weighted energy. By Parseval's theorem
            // (<https://en.wikipedia.org/wiki/Parseval%27s_theorem>) for RustFFT's
            // unnormalized IFFT with conjugate symmetry, the raw time-domain
            // RMS is `sqrt(2 × Σ |A(k)|²)`. We apply the same formula but
            // weight each bin's energy by the square of its A-weighting factor
            // (energy = amplitude², so weighting amplitude by W means weighting
            // energy by W²), then scale all bins so this A-weighted RMS matches
            // the target.
            //
            // This only runs on the first interval because subsequent intervals
            // preserve bin magnitudes (phase-only evolution).
            let ra1000 = r_a(1000.);
            let a_weighted_energy: f64 = pos[1..]
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let w = r_a((i + 1) as f64) / ra1000;
                    c.norm_sqr() * w * w
                })
                .sum::<f64>()
                * 2.0;
            let current_rms = a_weighted_energy.sqrt();
            let scale = target_rms / current_rms;
            for bin in pos[1..].iter_mut() {
                *bin *= scale;
            }
        } else {
            // Phases undergo a Brownian walk rather than being regenerated from
            // scratch. Fully re-randomizing would cause discontinuities at second
            // boundaries — especially at low frequencies, where a single cycle
            // can span the entire 1-second window and a phase jump creates an
            // audible click. Higher frequencies tolerate larger phase steps, hence
            // the `hz / MAX_FREQUENCY` scaling.
            for (hz, bin) in pos.iter_mut().enumerate().skip(1) {
                *bin *= Complex::from_polar(
                    1.,
                    (rng.random::<f64>() - 0.5)
                        * (hz as f64 / MAX_FREQUENCY as f64)
                        * std::f64::consts::FRAC_PI_2,
                );
            }
        }
        // For the IFFT output to be real-valued (as audio must be),
        // negative-frequency bins must be the complex conjugate of their
        // positive counterparts (Hermitian symmetry).
        // <https://en.wikipedia.org/wiki/Hermitian_function>
        for (bin, pos) in neg.iter_mut().skip(1).zip(pos.iter().rev()) {
            *bin = pos.conj();
        }
        // Nyquist bin: can only represent a signal alternating ±1 every sample.
        neg[0] = Complex::ZERO;
        c2r.process_immutable_with_scratch(&spectrum[..], &mut time[..], &mut scratch[..]);

        for sample in &time {
            // If Hermitian symmetry is correct, the IFFT output is purely real.
            // A non-negligible imaginary part indicates a spectrum setup bug.
            assert!(sample.im.abs() < 1., "{}", sample.im);
            let amplitude = sample.re.round();
            let amplitude = amplitude + amplitude * dampen;
            let amplitude = (amplitude as i64).clamp(i16::MIN as i64, i16::MAX as i64) as i16;
            dampen = (dampen + 0.0001).min(0.);
            out.write_all(&amplitude.to_le_bytes())?;
        }
    }

    out.flush()
}

fn write_chunk<T: IntoBytes + Immutable, W: Write>(
    fourcc: &[u8; 4],
    t: T,
    mut out: W,
) -> Result<(), std::io::Error> {
    out.write_all(fourcc)?;
    out.write_all(&(std::mem::size_of::<T>() as u32).to_le_bytes())?;
    t.write_to_io(&mut out)?;
    Ok(())
}
