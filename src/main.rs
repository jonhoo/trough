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
const MAX_FREQUENCY: u32 = 22050;
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
    let avg_amplitude = 8.;
    match args.color {
        Color::White => noise(args.duration, |spectrum| {
            for bin in spectrum {
                *bin =
                    Complex::from_polar(avg_amplitude, rng.random::<f64>() * std::f64::consts::TAU);
            }
        })?,
        Color::Pink => noise(args.duration, |spectrum| {
            let normalization = avg_amplitude * f64::sqrt(MAX_FREQUENCY as f64 / 2.);
            let power = normalization.powi(2);
            for (hz, bin) in spectrum.iter_mut().enumerate().skip(20) {
                *bin = Complex::from_polar(
                    (power / (hz + 1) as f64).sqrt(),
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        Color::Brownian => noise(args.duration, |spectrum| {
            // TODO: normalize this (and everything) by computing area under the curve of
            // amplitudes (maybe of power density instead?) and normalizing that to a particular
            // number. you know, instead of eyeballing "brownian is ~4x quieter than pink".
            let pink_normalization = avg_amplitude * f64::sqrt(MAX_FREQUENCY as f64 / 2.) * 4.;
            let pink_power = pink_normalization.powi(2);
            let pink_max_amplitude = (pink_power / 20.).sqrt();
            for (hz, bin) in spectrum.iter_mut().enumerate().skip(20) {
                *bin = Complex::from_polar(
                    ((1. / ((hz + 1) as f64)) / (1. / ((20 + 1) as f64))) * pink_max_amplitude,
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        Color::Blue => noise(args.duration, |spectrum| {
            let normalization = avg_amplitude / f64::sqrt(MAX_FREQUENCY as f64 / 2.);
            let power = normalization.powi(2);
            for (hz, bin) in spectrum.iter_mut().enumerate() {
                *bin = Complex::from_polar(
                    (power * (hz + 1) as f64).sqrt(),
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        Color::Violet => noise(args.duration, |spectrum| {
            let normalization = avg_amplitude / f64::sqrt(MAX_FREQUENCY as f64 / 2.) / 4.;
            let power = normalization.powi(2);
            for (hz, bin) in spectrum.iter_mut().enumerate() {
                *bin = Complex::from_polar(
                    power * (hz + 1) as f64,
                    rng.random::<f64>() * std::f64::consts::TAU,
                );
            }
        })?,
        Color::Grey => {
            // https://en.wikipedia.org/wiki/A-weighting
            let r_a = |hz: f64| {
                ((12194.0f64).powi(2) * hz.powi(4))
                    / ((hz.powi(2) + 20.6f64.powi(2))
                        * f64::sqrt(
                            (hz.powi(2) + 107.7f64.powi(2)) * (hz.powi(2) + 737.9f64.powi(2)),
                        )
                        * (hz.powi(2) + (12194.0f64).powi(2)))
            };
            let ra1000 = r_a(1000.);
            noise(args.duration, |spectrum| {
                for (hz, bin) in spectrum.iter_mut().enumerate().skip(20) {
                    // hz is 0-indexed within the closure, but the closure receives
                    // bins starting at frequency 1, so actual frequency is hz + 1.
                    let a_in_db =
                        20. * r_a((hz + 1) as f64).log10() - 20. * ra1000.log10();
                    let avg_in_db = 20. * avg_amplitude.log10();
                    let target_in_db = avg_in_db - a_in_db;
                    let a = 10.0f64.powf(target_in_db / 20.);
                    *bin = Complex::from_polar(a, rng.random::<f64>() * std::f64::consts::TAU);
                }
            })?
        }
    }

    Ok(())
}

fn noise(
    duration_in_seconds: u32,
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
    let mut dampen = -1.0;
    for interval in 0..duration_in_seconds {
        let (pos, neg) = spectrum.split_at_mut(SAMPLES_PER_SECOND as usize / 2);
        if interval == 0 {
            spectrum_setup(&mut pos[1..]);
            pos[0] = Complex::ZERO;
        } else {
            for (hz, bin) in pos.iter_mut().enumerate().skip(1) {
                *bin *= Complex::from_polar(
                    1.,
                    (rng.random::<f64>() - 0.5)
                        * (hz as f64 / MAX_FREQUENCY as f64)
                        * std::f64::consts::FRAC_PI_2,
                );
            }
        }
        // populate conjugates
        for (bin, pos) in neg.iter_mut().skip(1).zip(pos.iter().rev()) {
            *bin = pos.conj();
        }
        neg[0] = Complex::ZERO;
        c2r.process_immutable_with_scratch(&spectrum[..], &mut time[..], &mut scratch[..]);

        for sample in &time {
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
