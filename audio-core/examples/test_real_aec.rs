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
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

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

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

fn add_echo(mic: &[f32], ref_signal: &[f32], delay_ms: u32, attenuation: f32) -> Vec<f32> {
    let delay_samples = (delay_ms as usize * SAMPLE_RATE as usize) / 1000;
    let mut result = mic.to_vec();
    for i in delay_samples..mic.len() {
        if (i - delay_samples) < ref_signal.len() {
            result[i] += ref_signal[i - delay_samples] * attenuation;
        }
    }
    result
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

fn main() -> Result<()> {
    let mic_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (33).m4a");
    let ref_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (34).m4a");

    println!("=== Real Recording AEC Test ===");
    println!("NOTE: The two recordings were made at different times (1 min apart).");
    println!("      They are NOT synchronized - file 33 is mic, file 34 is speaker.");
    println!("      We're testing if AEC can handle arbitrary reference signals.\n");

    let mic_clean = decode_file(mic_file)?;
    let ref_signal = decode_file(ref_file)?;
    println!("Mic: {:.2}s, Ref: {:.2}s", 
        mic_clean.len() as f32 / SAMPLE_RATE as f32,
        ref_signal.len() as f32 / SAMPLE_RATE as f32);

    // Save originals
    save_wav("real_mic_clean.wav", &mic_clean, SAMPLE_RATE)?;
    save_wav("real_ref_clean.wav", &ref_signal, SAMPLE_RATE)?;

    // Test 1: AEC with silence as reference (should preserve mic)
    println!("\n=== Test 1: Ref=silence (AEC should pass mic through) ===");
    test_scenario(&mic_clean, &vec![0.0; mic_clean.len()], "aec_silence_ref.wav")?;

    // Test 2: Real ref as reference, no echo added
    // (This tests if AEC damages mic when given a "wrong" reference)
    println!("\n=== Test 2: Ref=real speaker (no echo added - AEC should pass mic through) ===");
    test_scenario(&mic_clean, &ref_signal[..mic_clean.len().min(ref_signal.len())].to_vec(), "aec_real_ref_no_echo.wav")?;

    // Test 3: Add echo and test cancellation
    println!("\n=== Test 3: Add 100ms echo @ -10dB ===");
    let ref_padded = pad_to_length(&ref_signal, mic_clean.len());
    let mic_with_echo = add_echo(&mic_clean, &ref_padded, 100, 0.316);
    test_scenario(&mic_with_echo, &ref_padded, "aec_with_echo_100ms.wav")?;

    Ok(())
}

fn test_scenario(mic: &[f32], reference: &[f32], output_name: &str) -> Result<()> {
    let processor = Processor::new(SAMPLE_RATE)?;
    let mut config = Config::default();
    config.echo_canceller = Some(EchoCanceller::Full {
        stream_delay_ms: None,
    });
    processor.set_config(config);

    let num_frames = mic.len() / FRAME_SIZE;
    let mut aec_output = Vec::with_capacity(mic.len());

    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;
        if end > reference.len() { break; }
        
        let ref_frame = reference[start..end].to_vec();
        let mic_frame = mic[start..end].to_vec();

        let mut ref_ch = vec![ref_frame];
        processor.process_render_frame(&mut ref_ch)?;

        let mut mic_ch = vec![mic_frame];
        processor.process_capture_frame(&mut mic_ch)?;

        aec_output.extend_from_slice(&mic_ch[0]);
    }

    let compare_len = mic.len().min(aec_output.len());
    
    // Skip first 1 second for AEC convergence
    let skip_samples = SAMPLE_RATE as usize;
    if compare_len > skip_samples {
        let correlation = calculate_correlation(&mic[..compare_len], &aec_output[..compare_len]);
        let aec_rms = calculate_rms(&aec_output[..compare_len]);
        let input_rms = calculate_rms(&mic[..compare_len]);
        
        // Also analyze after convergence
        let post_corr = calculate_correlation(&mic[skip_samples..compare_len], &aec_output[skip_samples..compare_len]);
        let post_aec_rms = calculate_rms(&aec_output[skip_samples..compare_len]);
        let post_input_rms = calculate_rms(&mic[skip_samples..compare_len]);

        println!("  Full:   RMS {:.4} -> {:.4} ({:+.2} dB), corr {:.4}", input_rms, aec_rms, 
            20.0 * (aec_rms / input_rms.max(0.0001)).log10(), correlation);
        println!("  After 1s: RMS {:.4} -> {:.4} ({:+.2} dB), corr {:.4}", post_input_rms, post_aec_rms,
            20.0 * (post_aec_rms / post_input_rms.max(0.0001)).log10(), post_corr);
        
        if post_corr > 0.9 {
            println!("  ✓ EXCELLENT (after convergence)");
        } else if post_corr > 0.7 {
            println!("  ✓ GOOD (after convergence)");
        } else {
            println!("  ✗ POOR (after convergence)");
        }
    } else {
        let correlation = calculate_correlation(&mic[..compare_len], &aec_output[..compare_len]);
        let aec_rms = calculate_rms(&aec_output[..compare_len]);
        let input_rms = calculate_rms(&mic[..compare_len]);
        println!("  Input  RMS:  {:.4}", input_rms);
        println!("  Output RMS:  {:.4}", aec_rms);
        println!("  Correlation:  {:.4}", correlation);
    }

    save_wav(output_name, &aec_output, SAMPLE_RATE)?;
    Ok(())
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
