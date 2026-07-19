use anyhow::{Context, Result};
use std::path::Path;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller, NoiseSuppression, NoiseSuppressionLevel, HighPassFilter, Pipeline, DownmixMethod};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const SAMPLE_RATE: u32 = 48000;
const FRAME_SIZE: usize = 480;

fn decode_file_stereo(path: &Path) -> Result<(Vec<f32>, Vec<f32>)> {
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
    let mut left: Vec<f32> = Vec::new();
    let mut right: Vec<f32> = Vec::new();
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
                            left.push(buf.chan(0)[i]);
                            right.push(if num_channels > 1 { buf.chan(1)[i] } else { buf.chan(0)[i] });
                        }
                    }
                    AudioBufferRef::S16(buf) => {
                        for i in 0..num_frames {
                            left.push(buf.chan(0)[i] as f32 / 32768.0);
                            right.push(if num_channels > 1 { buf.chan(1)[i] as f32 / 32768.0 } else { buf.chan(0)[i] as f32 / 32768.0 });
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
        let new_len = (left.len() as f64 * ratio) as usize;
        let mut l2 = Vec::with_capacity(new_len);
        let mut r2 = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx0 = src_idx as usize;
            let idx1 = (idx0 + 1).min(left.len().saturating_sub(1));
            let frac = (src_idx - idx0 as f64) as f32;
            l2.push(left[idx0] * (1.0 - frac) + left[idx1] * frac);
            r2.push(right[idx0] * (1.0 - frac) + right[idx1] * frac);
        }
        left = l2;
        right = r2;
    }
    Ok((left, right))
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
    println!("Decoding stereo...");
    let (near_l, near_r) = decode_file_stereo(near_file)?;
    let near_mono: Vec<f32> = near_l.iter().zip(near_r.iter()).map(|(l, r)| (l + r) * 0.5).collect();
    println!("  {} samples", near_l.len());

    let silence = vec![0.0f32; near_l.len()];
    let silence_stereo_r = vec![0.0f32; near_l.len()];

    println!("\n=== Test different AEC configs ===");

    // Config 1: Mono, no NS (my current setup)
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full { stream_delay_ms: None });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_mono.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_mono[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }
        let skip = SAMPLE_RATE as usize;
        let corr = calculate_correlation(&near_mono[skip..output.len()], &output[skip..]);
        println!("  Mono, NS=None: corr={:.4}", corr);
    }

    // Config 2: Mono, NS=High (PipeWire default)
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full { stream_delay_ms: None });
        config.noise_suppression = Some(NoiseSuppression {
            level: NoiseSuppressionLevel::High,
            analyze_linear_aec_output: false,
        });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_mono.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_mono[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }
        let skip = SAMPLE_RATE as usize;
        let corr = calculate_correlation(&near_mono[skip..output.len()], &output[skip..]);
        println!("  Mono, NS=High: corr={:.4}", corr);
    }

    // Config 3: Mono, NS=High + HPF (full PipeWire default)
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full { stream_delay_ms: None });
        config.noise_suppression = Some(NoiseSuppression {
            level: NoiseSuppressionLevel::High,
            analyze_linear_aec_output: false,
        });
        config.high_pass_filter = Some(HighPassFilter { apply_in_full_band: true });
        processor.set_config(config);

        let mut output = Vec::new();
        let num_frames = near_mono.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![silence[start..end].to_vec()];
            let mut mic_frame: Vec<Vec<f32>> = vec![near_mono[start..end].to_vec()];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output.extend_from_slice(&mic_frame[0]);
        }
        let skip = SAMPLE_RATE as usize;
        let corr = calculate_correlation(&near_mono[skip..output.len()], &output[skip..]);
        println!("  Mono, NS=High+HPF: corr={:.4}", corr);
    }

    // Config 4: STEREO capture + stereo render, multi_channel enabled (like PipeWire)
    {
        let processor = Processor::new(SAMPLE_RATE)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full { stream_delay_ms: None });
        config.noise_suppression = Some(NoiseSuppression {
            level: NoiseSuppressionLevel::High,
            analyze_linear_aec_output: false,
        });
        config.high_pass_filter = Some(HighPassFilter { apply_in_full_band: true });
        config.pipeline = Pipeline {
            maximum_internal_processing_rate: webrtc_audio_processing_config::PipelineProcessingRate::Max48000Hz,
            multi_channel_render: true,
            multi_channel_capture: true,
            capture_downmix_method: DownmixMethod::Average,
        };
        processor.set_config(config);

        let mut output_l = Vec::new();
        let mut output_r = Vec::new();
        let num_frames = near_l.len() / FRAME_SIZE;
        for i in 0..num_frames {
            let start = i * FRAME_SIZE;
            let end = start + FRAME_SIZE;
            let mut ref_frame: Vec<Vec<f32>> = vec![
                silence[start..end].to_vec(),
                silence_stereo_r[start..end].to_vec(),
            ];
            let mut mic_frame: Vec<Vec<f32>> = vec![
                near_l[start..end].to_vec(),
                near_r[start..end].to_vec(),
            ];
            processor.process_render_frame(&mut ref_frame)?;
            processor.process_capture_frame(&mut mic_frame)?;
            output_l.extend_from_slice(&mic_frame[0]);
            output_r.extend_from_slice(&mic_frame[1]);
        }
        let skip = SAMPLE_RATE as usize;
        let corr_l = calculate_correlation(&near_l[skip..output_l.len()], &output_l[skip..]);
        let corr_r = calculate_correlation(&near_r[skip..output_r.len()], &output_r[skip..]);
        let out_mono: Vec<f32> = output_l.iter().zip(output_r.iter()).map(|(l, r)| (l + r) * 0.5).collect();
        let corr_mono = calculate_correlation(&near_mono[skip..out_mono.len()], &out_mono[skip..]);
        println!("  Stereo MC, NS=High+HPF: corr_L={:.4}, corr_R={:.4}, corr_mono={:.4}", corr_l, corr_r, corr_mono);
    }

    Ok(())
}
