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

fn main() -> Result<()> {
    let near_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (38).m4a");
    println!("Decoding near-end...");
    let near_end = decode_file(near_file)?;
    println!("  {} samples", near_end.len());

    let silence = vec![0.0f32; near_end.len()];

    // Test 1: No processing at all (echo_canceller = None)
    println!("\n=== Test 1: echo_canceller=None (should be passthrough) ===");
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = None;
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_end.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_end[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }

        let corr = calculate_correlation(&near_end[..output.len()], &output);
        let in_rms = calculate_rms(&near_end[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);

        // Check first 10 samples
        println!("  Input[0..10]:  {:?}", &near_end[..10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
        println!("  Output[0..10]: {:?}", &output[..10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
    }

    // Test 2: AEC enabled with silence ref
    println!("\n=== Test 2: echo_canceller=Full, ref=silence ===");
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_end.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_end[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }

        let corr = calculate_correlation(&near_end[..output.len()], &output);
        let in_rms = calculate_rms(&near_end[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);

        // Check correlation after 1s
        let skip = SAMPLE_RATE as usize;
        if output.len() > skip {
            let corr_post = calculate_correlation(&near_end[skip..output.len()], &output[skip..]);
            println!("  Correlation after 1s: {:.6}", corr_post);
        }

        // Check samples at various points
        let mid = output.len() / 2;
        println!("  Input[mid..mid+10]:  {:?}", &near_end[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
        println!("  Output[mid..mid+10]: {:?}", &output[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
    }

    // Test 3: AEC with 2-channel render (stereo) like recording.rs example
    println!("\n=== Test 3: AEC, render=2ch, capture=1ch, ref=silence ===");
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        processor.set_config(config);

        let silence_2ch = vec![0.0f32; near_end.len() * 2];
        let mut output = Vec::new();
        let num_frames = near_end.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            // 2-channel render (like recording.rs example)
            let mut ref_frame: Vec<Vec<f32>> = vec![
                silence[start..end].to_vec(),
                silence[start..end].to_vec(),
            ];
            // 1-channel capture
            let mut mic_frame: Vec<Vec<f32>> = vec![near_end[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }

        let corr = calculate_correlation(&near_end[..output.len()], &output);
        let in_rms = calculate_rms(&near_end[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);
        let mid = output.len() / 2;
        println!("  Input[mid..mid+10]:  {:?}", &near_end[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
        println!("  Output[mid..mid+10]: {:?}", &output[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
    }

    // Test 4: AEC with pure sine wave (1kHz), ref=silence, 1ch
    println!("\n=== Test 4: AEC, 1kHz sine, ref=silence, 1ch ===");
    {
        let sine = generate_sine(1000.0, SAMPLE_RATE, near_end.len(), 0.5);
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = sine.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![sine[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }

        let corr = calculate_correlation(&sine[..output.len()], &output);
        let in_rms = calculate_rms(&sine[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);
        let mid = output.len() / 2;
        println!("  Input[mid..mid+10]:  {:?}", &sine[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
        println!("  Output[mid..mid+10]: {:?}", &output[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
    }

    // Test 5: Mobile AEC mode (AECM) instead of Full (AEC3)
    println!("\n=== Test 5: Mobile AEC (AECM), ref=silence ===");
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Mobile {
            stream_delay_ms: 0,
        });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_end.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_end[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }

        let corr = calculate_correlation(&near_end[..output.len()], &output);
        let in_rms = calculate_rms(&near_end[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);
        let mid = output.len() / 2;
        println!("  Input[mid..mid+10]:  {:?}", &near_end[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
        println!("  Output[mid..mid+10]: {:?}", &output[mid..mid+10].iter().map(|x| (x * 10000.0) as i32).collect::<Vec<_>>());
    }

    // Test 6: Full AEC with high_pass_filter enabled
    println!("\n=== Test 6: Full AEC + high_pass_filter, ref=silence ===");
    {
        use webrtc_audio_processing_config::HighPassFilter;
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        config.high_pass_filter = Some(HighPassFilter {
            apply_in_full_band: true,
        });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_end.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_end[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }

        let corr = calculate_correlation(&near_end[..output.len()], &output);
        let in_rms = calculate_rms(&near_end[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);
    }

    // Test 7: Full AEC with stats monitoring
    println!("\n=== Test 7: Full AEC with stats, ref=silence ===");
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_end.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_end[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);

            // Print stats every 100 frames (1 second)
            if i % 100 == 99 {
                let stats = processor.get_stats();
                println!("  Frame {}: {:?}", i + 1, stats);
            }
        }

        let corr = calculate_correlation(&near_end[..output.len()], &output);
        let in_rms = calculate_rms(&near_end[..output.len()]);
        let out_rms = calculate_rms(&output);
        println!("  Correlation: {:.6}", corr);
        println!("  RMS: {:.6} -> {:.6}", in_rms, out_rms);
        println!("  Final stats: {:?}", processor.get_stats());
    }

    Ok(())
}

fn generate_sine(freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    use std::f32::consts::PI;
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * PI * freq * t).sin() * amplitude
        })
        .collect()
}
