use anyhow::Result;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller};
use std::f32::consts::PI;

const SAMPLE_RATE: u32 = 48000;
const FRAME_SIZE: usize = 480;

fn generate_sine(freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * PI * freq * t).sin() * amplitude
        })
        .collect()
}

/// Generate speech-like signal (multiple harmonics + AM modulation)
fn generate_speech_like(base_freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let h1 = (2.0 * PI * base_freq * t).sin();
            let h2 = 0.6 * (2.0 * PI * base_freq * 2.0 * t).sin();
            let h3 = 0.4 * (2.0 * PI * base_freq * 3.0 * t).sin();
            let h4 = 0.25 * (2.0 * PI * base_freq * 4.0 * t).sin();
            let envelope = 0.5 + 0.5 * (2.0 * PI * 4.0 * t).sin();
            // Center the envelope around zero to avoid DC offset
            let centered_envelope = envelope - 0.5;
            centered_envelope * (h1 + h2 + h3 + h4) * amplitude
        })
        .collect()
}

/// Music-like: chord with multiple frequencies
fn generate_music(sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // C major chord: C(261.63), E(329.63), G(392.00)
            let c = (2.0 * PI * 261.63 * t).sin();
            let e = (2.0 * PI * 329.63 * t).sin();
            let g = (2.0 * PI * 392.00 * t).sin();
            (c * 0.4 + e * 0.3 + g * 0.3) * amplitude
        })
        .collect()
}

fn add_echo(mic: &[f32], ref_signal: &[f32], delay_samples: usize, attenuation: f32) -> Vec<f32> {
    let mut result = mic.to_vec();
    for i in delay_samples..mic.len() {
        if (i - delay_samples) < ref_signal.len() {
            result[i] += ref_signal[i - delay_samples] * attenuation;
        }
    }
    result
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

fn run_aec(mic: &[f32], reference: &[f32]) -> Result<Vec<f32>> {
    let processor = Processor::new(SAMPLE_RATE)?;
    let mut config = Config::default();
    config.echo_canceller = Some(EchoCanceller::Full {
        stream_delay_ms: None,
    });
    processor.set_config(config);

    let num_frames = mic.len() / FRAME_SIZE;
    let mut output = Vec::with_capacity(mic.len());

    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;
        if end > reference.len() { break; }
        
        let ref_frame = reference[start..end].to_vec();
        let mut mic_frame = mic[start..end].to_vec();

        // WebRTC expects non-interleaved format: Vec<Vec<f32>> where inner Vec is per-channel
        // For mono, this is a single channel
        let ref_channels = vec![ref_frame];
        let mut mic_channels = vec![mic_frame];
        
        processor.process_render_frame(&mut ref_channels.clone())?;
        processor.process_capture_frame(&mut mic_channels)?;

        output.extend_from_slice(&mic_channels[0]);
    }

    Ok(output)
}

fn analyze(name: &str, near_end: &[f32], aec_out: &[f32], mic_in: &[f32]) {
    let compare_len = near_end.len().min(aec_out.len());
    let skip = SAMPLE_RATE as usize; // Skip 1s for convergence

    println!("\n=== {} ===", name);

    let post_corr = if compare_len > skip {
        calculate_correlation(&near_end[skip..compare_len], &aec_out[skip..compare_len])
    } else { 0.0 };
    let post_aec_rms = if compare_len > skip { calculate_rms(&aec_out[skip..compare_len]) } else { 0.0 };
    let post_near_rms = if compare_len > skip { calculate_rms(&near_end[skip..compare_len]) } else { 0.0 };
    let post_mic_rms = if compare_len > skip { calculate_rms(&mic_in[skip..compare_len]) } else { 0.0 };

    // Also compute correlation with -aec_out to see if signal is inverted
    let post_neg_corr = if compare_len > skip {
        let neg_aec: Vec<f32> = aec_out[skip..compare_len].iter().map(|x| -x).collect();
        calculate_correlation(&near_end[skip..compare_len], &neg_aec)
    } else { 0.0 };
    
    // Compute correlation with |aec_out|
    let post_abs_corr = if compare_len > skip {
        let abs_aec: Vec<f32> = aec_out[skip..compare_len].iter().map(|x| x.abs()).collect();
        calculate_correlation(&near_end[skip..compare_len].iter().map(|x| x.abs()).collect::<Vec<_>>(), &abs_aec)
    } else { 0.0 };

    println!("  After 1s convergence:");
    println!("    Mic  RMS: {:.4} -> AEC RMS: {:.4} ({:+.2} dB)",
        post_mic_rms, post_aec_rms, 20.0 * (post_aec_rms / post_mic_rms.max(0.0001)).log10());
    println!("    Near RMS: {:.4}, Correlation: {:.4}", post_near_rms, post_corr);
    println!("    Correlation with -AEC: {:.4} (inverted?)", post_neg_corr);
    println!("    Correlation with |AEC|: {:.4} (envelope?)", post_abs_corr);

    // Check if |corr| is high
    let abs_corr = post_corr.abs();
    if abs_corr > 0.9 {
        if post_corr > 0.0 {
            println!("  ✓✓ EXCELLENT: High positive correlation");
        } else {
            println!("  ⚠ INVERTED: High correlation but inverted (multiply by -1)");
        }
    } else if post_neg_corr.abs() > 0.9 {
        println!("  ⚠ INVERTED: Need to invert signal");
    } else if abs_corr > 0.7 {
        println!("  ✓ GOOD");
    } else {
        println!("  ✗ POOR: Low correlation");
    }
}

fn main() -> Result<()> {
    let duration_secs = 5.0;
    let total_samples = (SAMPLE_RATE as f32 * duration_secs) as usize;

    println!("=== Realistic AEC Test ===");
    println!("Speaker: music (C major chord) | Mic: human voice (speech-like)");
    println!("Sample rate: {} Hz, Duration: {:.1}s", SAMPLE_RATE, duration_secs);

    // === Scenario A: Music from speaker, human voice from mic ===
    // This is a realistic video call scenario
    let speaker = generate_music(SAMPLE_RATE, total_samples, 0.5);
    let voice = generate_speech_like(250.0, SAMPLE_RATE, total_samples, 0.5);

    // Debug: check first samples
    println!("Voice first 10 samples: {:?}", &voice[..10].iter().map(|x| (x * 1000.0) as i32).collect::<Vec<_>>());
    println!("Speaker first 10 samples: {:?}", &speaker[..10].iter().map(|x| (x * 1000.0) as i32).collect::<Vec<_>>());

    // Save originals
    save_wav("synth_speaker.wav", &speaker, SAMPLE_RATE)?;
    save_wav("synth_voice.wav", &voice, SAMPLE_RATE)?;

    // Test 1: Voice only, speaker=silence (baseline)
    let silence = vec![0.0f32; total_samples];
    let mic1 = voice.clone();
    let aec1 = run_aec(&mic1, &silence)?;
    analyze("Test 1: Voice only, ref=silence (baseline)", &voice, &aec1, &mic1);
    save_wav("synth_aec1.wav", &aec1, SAMPLE_RATE)?;

    // Test 2: Voice + echo 100ms -10dB
    let echo_delay_2 = 100 * SAMPLE_RATE as usize / 1000;
    let mic2 = add_echo(&voice, &speaker, echo_delay_2, 0.316);
    let aec2 = run_aec(&mic2, &speaker)?;
    analyze("Test 2: Voice + music-echo(100ms,-10dB)", &voice, &aec2, &mic2);
    save_wav("synth_aec2.wav", &aec2, SAMPLE_RATE)?;
    save_wav("synth_mic2.wav", &mic2, SAMPLE_RATE)?;

    // Test 3: Voice + echo 200ms -15dB (more realistic echo level)
    let echo_delay_3 = 200 * SAMPLE_RATE as usize / 1000;
    let mic3 = add_echo(&voice, &speaker, echo_delay_3, 0.178); // -15dB
    let aec3 = run_aec(&mic3, &speaker)?;
    analyze("Test 3: Voice + music-echo(200ms,-15dB)", &voice, &aec3, &mic3);
    save_wav("synth_aec3.wav", &aec3, SAMPLE_RATE)?;
    save_wav("synth_mic3.wav", &mic3, SAMPLE_RATE)?;

    // Test 4: Only music-echo, no voice
    let mic4 = add_echo(&vec![0.0; total_samples], &speaker, echo_delay_2, 0.316);
    let aec4 = run_aec(&mic4, &speaker)?;
    let echo_only = add_echo(&vec![0.0; total_samples], &speaker, echo_delay_2, 0.316);
    let aec4_rms = calculate_rms(&aec4);
    let echo4_rms = calculate_rms(&echo_only);
    println!("\n=== Test 4: Only music-echo (no voice), ref=music ===");
    println!("  Echo RMS: {:.4}, AEC output RMS: {:.4}", echo4_rms, aec4_rms);
    println!("  Suppression: {:.2} dB", 20.0 * (aec4_rms / echo4_rms.max(0.0001)).log10());
    save_wav("synth_aec4.wav", &aec4, SAMPLE_RATE)?;

    println!("\n=== Files saved ===");
    println!("  synth_speaker.wav  - Synthetic music (speaker output)");
    println!("  synth_voice.wav    - Synthetic voice (near-end)");
    println!("  synth_aec1.wav     - AEC: voice only, ref=silence");
    println!("  synth_aec2.wav     - AEC: voice+echo 100ms, ref=music");
    println!("  synth_aec3.wav     - AEC: voice+echo 200ms, ref=music");
    println!("  synth_aec4.wav     - AEC: only echo, ref=music");
    println!("  synth_mic2.wav     - Mic input for test 2");
    println!("  synth_mic3.wav     - Mic input for test 3");

    Ok(())
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
