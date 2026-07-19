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

/// Cross-correlation: find the lag where a and b match best
fn cross_correlation(a: &[f32], b: &[f32], max_lag: usize) -> (usize, f32) {
    let mut best_lag = 0;
    let mut best_corr = -1.0f32;
    let len = a.len().min(b.len());
    if len == 0 { return (0, 0.0); }

    for lag in 0..max_lag.min(len) {
        let mut num = 0.0f32;
        let mut den_a = 0.0f32;
        let mut den_b = 0.0f32;
        let n = len - lag;
        if n == 0 { break; }

        let mean_a: f32 = a[..n].iter().sum::<f32>() / n as f32;
        let mean_b: f32 = b[lag..lag + n].iter().sum::<f32>() / n as f32;

        for i in 0..n {
            let da = a[i] - mean_a;
            let db = b[lag + i] - mean_b;
            num += da * db;
            den_a += da * da;
            den_b += db * db;
        }
        let den = (den_a * den_b).sqrt();
        let corr = if den < 1e-10 { 0.0 } else { num / den };
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }
    (best_lag, best_corr)
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

fn main() -> Result<()> {
    let near_file = Path::new(r"C:\Users\Key\Documents\录音\录音 (38).m4a");
    println!("Decoding...");
    let near_end = decode_file(near_file)?;
    println!("  {} samples", near_end.len());

    let silence = vec![0.0f32; near_end.len()];

    println!("\n=== AEC output analysis (ref=silence) ===");
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full { stream_delay_ms: None });
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

        let skip = SAMPLE_RATE as usize; // skip 1s for convergence
        let seg_len = 4800.min(output.len() - skip);
        let input_seg = &near_end[skip..skip + seg_len];
        let output_seg = &output[skip..skip + seg_len];

        println!("  Input  RMS: {:.6}", calculate_rms(input_seg));
        println!("  Output RMS: {:.6}", calculate_rms(output_seg));

        let (lag, corr) = cross_correlation(input_seg, output_seg, 960);
        println!("  Best match at lag={} samples ({:.1}ms), correlation={:.4}", lag, lag as f32 * 1000.0 / SAMPLE_RATE as f32, corr);

        // Find a segment with actual voice content (high RMS)
        let window = 4800;
        let mut best_pos = 0;
        let mut best_rms = 0.0f32;
        for pos in (skip..output.len() - window).step_by(480) {
            let rms = calculate_rms(&near_end[pos..pos + window]);
            if rms > best_rms {
                best_rms = rms;
                best_pos = pos;
            }
        }

        println!("\n  Voice segment at pos={} (RMS={:.6}):", best_pos, best_rms);
        let voice_in = &near_end[best_pos..best_pos + window];
        let voice_out = &output[best_pos..best_pos + window];
        let (vlag, vcorr) = cross_correlation(voice_in, voice_out, 960);
        println!("  Voice best match at lag={} samples ({:.1}ms), correlation={:.4}", vlag, vlag as f32 * 1000.0 / SAMPLE_RATE as f32, vcorr);

        // Show samples at the best lag
        let show = 30;
        println!("  Input[0..{}]:  {:?}", show, &voice_in[..show].iter().map(|x| (x * 1000.0) as i32).collect::<Vec<_>>());
        println!("  Output[lag..lag+{}]: {:?}", show, &voice_out[vlag..vlag + show.min(voice_out.len() - vlag)].iter().map(|x| (x * 1000.0) as i32).collect::<Vec<_>>());

        // RMS comparison
        println!("  Voice Input  RMS: {:.6}", calculate_rms(voice_in));
        println!("  Voice Output RMS: {:.6}", calculate_rms(voice_out));
    }

    Ok(())
}
