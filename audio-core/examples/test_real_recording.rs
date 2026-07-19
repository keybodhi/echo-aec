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
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open: {:?}", path))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let hint = Hint::new();
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;
    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("No audio track")?
        .clone();
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

    println!("  {} Hz, {} channels", sample_rate, channels);

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

    // Resample to 48kHz if needed
    if sample_rate != SAMPLE_RATE {
        println!("  Resampling {} -> {} Hz", sample_rate, SAMPLE_RATE);
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

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
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

fn add_echo(mic: &[f32], echo_source: &[f32], delay_samples: usize, attenuation: f32) -> Vec<f32> {
    let mut result = mic.to_vec();
    for i in delay_samples..mic.len() {
        if (i - delay_samples) < echo_source.len() {
            result[i] += echo_source[i - delay_samples] * attenuation;
        }
    }
    result
}

fn pad_to_length(signal: &[f32], length: usize) -> Vec<f32> {
    if signal.len() >= length {
        signal[..length].to_vec()
    } else {
        let mut padded = signal.to_vec();
        padded.resize(length, 0.0);
        padded
    }
}

fn run_aec(mic_in: &[f32], far_end: &[f32], delay_ms: Option<u16>) -> Result<Vec<f32>> {
    let processor = Processor::new(SAMPLE_RATE)?;
    let mut config = Config::default();
    config.echo_canceller = Some(EchoCanceller::Full {
        stream_delay_ms: delay_ms,
    });
    processor.set_config(config);

    let num_frames = mic_in.len() / FRAME_SIZE;
    let mut aec_out = Vec::with_capacity(mic_in.len());

    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;
        if end > far_end.len() { break; }

        let mut ref_frame: Vec<Vec<f32>> = vec![far_end[start..end].to_vec()];
        let mut mic_frame: Vec<Vec<f32>> = vec![mic_in[start..end].to_vec()];

        processor.process_render_frame(&mut ref_frame)?;
        processor.process_capture_frame(&mut mic_frame)?;

        aec_out.extend_from_slice(&mic_frame[0]);
    }

    Ok(aec_out)
}

fn save_wav(filename: &str, samples: &[f32], sample_rate: u32) -> Result<()> {
    use std::fs::File;
    use std::io::Write;
    let mut file = File::create(filename)?;
    let num_samples = samples.len() as u32;
    let data_size = num_samples * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_size).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;
    for &sample in samples {
        let clamped = sample.max(-1.0).min(1.0);
        let i16_sample = (clamped * 32767.0) as i16;
        file.write_all(&i16_sample.to_le_bytes())?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let near_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (38).m4a");
    let far_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (39).m4a");

    println!("=== Real Recording AEC Test ===");
    println!("Near-end (38): {:?}", near_file);
    let near_end = decode_file(near_file)?;
    println!("Far-end  (39): {:?}", far_file);
    let far_end_raw = decode_file(far_file)?;

    println!("\nNear-end: {} samples ({:.2}s)", near_end.len(), near_end.len() as f32 / SAMPLE_RATE as f32);
    println!("Far-end:  {} samples ({:.2}s)", far_end_raw.len(), far_end_raw.len() as f32 / SAMPLE_RATE as f32);

    // Save decoded originals
    save_wav("real_near_end.wav", &near_end, SAMPLE_RATE)?;
    save_wav("real_far_end.wav", &far_end_raw, SAMPLE_RATE)?;

    // Pad far_end to at least match near_end length
    let far_end = pad_to_length(&far_end_raw, near_end.len());

    // Test scenarios with different echo delays
    let scenarios = vec![
        ("mic_only_ref_silence", 0u32, 0.0f32, true),  // mic=voice, ref=silence
        ("mic_only_ref_speaker", 0u32, 0.0f32, false), // mic=voice, ref=speaker
        ("echo_50ms_-10dB", 50u32, 0.316f32, false),
        ("echo_100ms_-10dB", 100u32, 0.316f32, false),
        ("echo_200ms_-10dB", 200u32, 0.316f32, false),
    ];

    for (name, delay_ms, attenuation, use_silence_ref) in &scenarios {
        println!("\n=== Scenario: {} ===", name);

        let delay_samples = (*delay_ms as usize) * SAMPLE_RATE as usize / 1000;
        let mic_in = if *attenuation == 0.0 {
            near_end.clone()
        } else {
            add_echo(&near_end, &far_end, delay_samples, *attenuation)
        };

        // Use silence as reference for diagnostic test
        let ref_to_use = if *use_silence_ref {
            vec![0.0f32; far_end.len()]
        } else {
            far_end.clone()
        };

        // Save mic input
        save_wav(&format!("real_mic_{}.wav", name), &mic_in, SAMPLE_RATE)?;

        // Test with different AEC delay settings
        let aec_delays: Vec<Option<u16>> = vec![None, Some(100)];

        for aec_delay in &aec_delays {
            let aec_out = run_aec(&mic_in, &ref_to_use, *aec_delay)?;

            // Skip first 1 second for convergence
            let skip = SAMPLE_RATE as usize;
            let compare_len = near_end.len().min(aec_out.len());

            let (corr, input_rms, aec_rms) = if compare_len > skip {
                (
                    calculate_correlation(&near_end[skip..compare_len], &aec_out[skip..compare_len]),
                    calculate_rms(&mic_in[skip..compare_len]),
                    calculate_rms(&aec_out[skip..compare_len]),
                )
            } else {
                (0.0, 0.0, 0.0)
            };

            let delay_str = match aec_delay {
                None => "auto".to_string(),
                Some(d) => format!("{}ms", d),
            };

            println!("  AEC delay={:6}: corr={:.4}, RMS {:.4}->{:.4} ({:+.1} dB)",
                delay_str, corr, input_rms, aec_rms,
                20.0 * (aec_rms / input_rms.max(0.0001)).log10());

            save_wav(&format!("real_aec_{}_delay{}.wav", name, delay_str), &aec_out, SAMPLE_RATE)?;
        }
    }

    println!("\n=== Files saved ===");
    println!("  real_near_end.wav     - Near-end (clean voice)");
    println!("  real_far_end.wav      - Far-end (speaker)");
    println!("  real_mic_*.wav        - Mic input for each scenario");
    println!("  real_aec_*.wav        - AEC output for each scenario");

    Ok(())
}
