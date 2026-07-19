use anyhow::{Context, Result};
use std::path::Path;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const SAMPLE_RATE: u32 = 48000;
const FRAME_SIZE: usize = 480;

fn decode_file(path: &Path) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let hint = Hint::new();
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?;
    let mut format = probed.format;
    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("No track")?
        .clone();
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;
    let mut all_samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::ResetRequired) => { decoder.reset(); continue; }
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id { continue; }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let num_channels = decoded.spec().channels.count();
                let num_frames = decoded.capacity();
                match decoded {
                    AudioBufferRef::F32(buf) => {
                        for i in 0..num_frames {
                            let mut sum = 0.0f32;
                            for ch in 0..num_channels { sum += buf.chan(ch)[i]; }
                            all_samples.push(sum / num_channels as f32);
                        }
                    }
                    AudioBufferRef::S16(buf) => {
                        for i in 0..num_frames {
                            let mut sum = 0.0f32;
                            for ch in 0..num_channels { sum += buf.chan(ch)[i] as f32 / 32768.0; }
                            all_samples.push(sum / num_channels as f32);
                        }
                    }
                    _ => continue,
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    if sample_rate != SAMPLE_RATE {
        let ratio = SAMPLE_RATE as f64 / sample_rate as f64;
        let new_len = (all_samples.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx0 = src_idx as usize;
            let idx1 = (idx0 + 1).min(all_samples.len().saturating_sub(1));
            let frac = (src_idx - idx0 as f64) as f32;
            resampled.push(all_samples[idx0] * (1.0 - frac) + all_samples[idx1] * frac);
        }
        all_samples = resampled;
    }
    Ok(all_samples)
}

fn calculate_correlation(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 { return 0.0; }
    let mean_a: f32 = a[..len].iter().sum::<f32>() / len as f32;
    let mean_b: f32 = b[..len].iter().sum::<f32>() / len as f32;
    let mut num = 0.0f32; let mut den_a = 0.0f32; let mut den_b = 0.0f32;
    for i in 0..len {
        let da = a[i] - mean_a; let db = b[i] - mean_b;
        num += da * db; den_a += da * da; den_b += db * db;
    }
    let den = (den_a * den_b).sqrt();
    if den < 1e-10 { return 0.0; }
    num / den
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

fn run_aec_test(name: &str, input: &[f32]) -> Result<()> {
    let silence = vec![0.0f32; input.len()];
    let processor = Processor::new(SAMPLE_RATE)?;
    let mut config = Config::default();
    config.echo_canceller = Some(EchoCanceller::Full { stream_delay_ms: None });
    processor.set_config(config);

    let mut output = Vec::new();
    let num_frames = input.len() / FRAME_SIZE;
    for i in 0..num_frames {
        let start = i * FRAME_SIZE;
        let end = start + FRAME_SIZE;
        let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
        let mut mic_frame: Vec<Vec<f32>> = vec![input[start..end].to_vec()];
        processor.process_render_frame(&mut ref_frame)?;
        processor.process_capture_frame(&mut mic_frame)?;
        output.extend_from_slice(&mic_frame[0]);
    }

    let skip = SAMPLE_RATE as usize;
    let corr = if output.len() > skip {
        calculate_correlation(&input[skip..output.len()], &output[skip..])
    } else { 0.0 };
    let in_rms = calculate_rms(&input[..output.len()]);
    let out_rms = calculate_rms(&output);
    println!("  {}: corr={:.4}, RMS {:.4}->{:.4}", name, corr, in_rms, out_rms);
    Ok(())
}

fn main() -> Result<()> {
    let near_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (38).m4a");
    println!("Decoding...");
    let near_end = decode_file(near_file)?;
    let orig_rms = calculate_rms(&near_end);
    println!("  {} samples, RMS={:.4}", near_end.len(), orig_rms);

    println!("\n=== Energy level tests (ref=silence) ===");

    // Original signal
    run_aec_test("Original (RMS=0.076)", &near_end)?;

    // Amplified signals
    for gain in [2.0f32, 5.0f32, 10.0f32] {
        let amplified: Vec<f32> = near_end.iter().map(|s| (s * gain).max(-1.0).min(1.0)).collect();
        let rms = calculate_rms(&amplified);
        run_aec_test(&format!("Gain={}x (RMS={:.3})", gain, rms), &amplified)?;
    }

    // Pure tone for comparison
    use std::f32::consts::PI;
    let tone: Vec<f32> = (0..near_end.len()).map(|i| {
        let t = i as f32 / SAMPLE_RATE as f32;
        (2.0 * PI * 250.0 * t).sin() * 0.35
    }).collect();
    let tone_rms = calculate_rms(&tone);
    run_aec_test(&format!("250Hz tone (RMS={:.3})", tone_rms), &tone)?;

    // 1000Hz tone (period=48, divides FRAME_SIZE=480 evenly)
    let tone_1k: Vec<f32> = (0..near_end.len()).map(|i| {
        let t = i as f32 / SAMPLE_RATE as f32;
        (2.0 * PI * 1000.0 * t).sin() * 0.35
    }).collect();
    let tone_1k_rms = calculate_rms(&tone_1k);
    run_aec_test(&format!("1000Hz tone (RMS={:.3})", tone_1k_rms), &tone_1k)?;

    // 1000Hz tone with AM modulation (simulating voice envelope)
    let tone_am: Vec<f32> = (0..near_end.len()).map(|i| {
        let t = i as f32 / SAMPLE_RATE as f32;
        let envelope = 0.5 + 0.5 * (2.0 * PI * 4.0 * t).sin();
        envelope * (2.0 * PI * 1000.0 * t).sin() * 0.35
    }).collect();
    let tone_am_rms = calculate_rms(&tone_am);
    run_aec_test(&format!("1000Hz AM (RMS={:.3})", tone_am_rms), &tone_am)?;

    // Voice + 1000Hz tone
    let mixed: Vec<f32> = near_end.iter().zip(tone_1k.iter()).map(|(v, t)| (v + t * 0.5).max(-1.0).min(1.0)).collect();
    let mixed_rms = calculate_rms(&mixed);
    run_aec_test(&format!("Voice + 1000Hz (RMS={:.3})", mixed_rms), &mixed)?;

    Ok(())
}
