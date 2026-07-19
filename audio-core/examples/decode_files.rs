use anyhow::{Context, Result};
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decode an audio file to mono f32 samples at the target sample rate.
pub fn decode_file(path: &Path, target_sample_rate: u32) -> Result<(Vec<f32>, u32, usize)> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open file: {:?}", path))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .with_context(|| "Failed to probe audio format")?;

    let mut format = probed.format;
    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("No audio track found")?
        .clone();

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

    println!("  Sample rate: {} Hz, Channels: {}", sample_rate, channels);

    let dec_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .with_context(|| "Failed to create decoder")?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let num_channels = spec.channels.count();
                let num_frames = decoded.capacity();

                match decoded {
                    AudioBufferRef::F32(buf) => {
                        for i in 0..num_frames {
                            let mut sum = 0.0f32;
                            for ch in 0..num_channels {
                                sum += buf.chan(ch)[i];
                            }
                            all_samples.push(sum / num_channels as f32);
                        }
                    }
                    AudioBufferRef::S16(buf) => {
                        for i in 0..num_frames {
                            let mut sum = 0.0f32;
                            for ch in 0..num_channels {
                                sum += buf.chan(ch)[i] as f32 / 32768.0;
                            }
                            all_samples.push(sum / num_channels as f32);
                        }
                    }
                    _ => {
                        println!("  Unsupported audio format");
                        continue;
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    let original_len = all_samples.len();
    let original_sr = sample_rate;
    let original_ch = channels;

    if sample_rate != target_sample_rate {
        println!("  Resampling from {} to {} Hz", sample_rate, target_sample_rate);
        let ratio = target_sample_rate as f64 / sample_rate as f64;
        let new_len = (original_len as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx0 = src_idx as usize;
            let idx1 = (idx0 + 1).min(original_len.saturating_sub(1));
            let frac = (src_idx - idx0 as f64) as f32;
            let sample = all_samples[idx0] * (1.0 - frac) + all_samples[idx1] * frac;
            resampled.push(sample);
        }
        all_samples = resampled;
    }

    println!("  Decoded {} samples ({:.2}s)", all_samples.len(), all_samples.len() as f32 / target_sample_rate as f32);

    Ok((all_samples, original_sr, original_ch))
}

fn main() -> Result<()> {
    let file1 = Path::new(r"C:\Users\Key\Documents\录音\录音 (33).m4a");
    let file2 = Path::new(r"C:\Users\Key\Documents\录音\录音 (34).m4a");

    println!("=== Decoding audio files ===");
    println!("File 1 (near-end, mic): {:?}", file1);
    let (mic_signal, sr1, ch1) = decode_file(file1, 48000)?;

    println!("\nFile 2 (far-end, speaker): {:?}", file2);
    let (ref_signal, sr2, ch2) = decode_file(file2, 48000)?;

    println!("\n=== File info ===");
    println!("Mic: {} Hz, {} channels, {} samples ({:.2}s)", sr1, ch1, mic_signal.len(), mic_signal.len() as f32 / 48000.0);
    println!("Ref: {} Hz, {} channels, {} samples ({:.2}s)", sr2, ch2, ref_signal.len(), ref_signal.len() as f32 / 48000.0);

    save_wav("decoded_mic.wav", &mic_signal, 48000)?;
    save_wav("decoded_ref.wav", &ref_signal, 48000)?;

    println!("\nSaved decoded WAV files:");
    println!("  decoded_mic.wav");
    println!("  decoded_ref.wav");

    Ok(())
}

fn save_wav(filename: &str, samples: &[f32], sample_rate: u32) -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create(filename)?;
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * 2;
    let data_size = num_samples * 2;

    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_size).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
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
