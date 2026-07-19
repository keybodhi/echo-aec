use anyhow::Result;
use std::f32::consts::PI;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller};

const SAMPLE_RATE: u32 = 48000;
const FRAME_SIZE: usize = 480;

/// Generate sine wave
fn generate_sine(freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * PI * freq * t).sin() * amplitude
        })
        .collect()
}

/// Generate speech-like signal (multiple harmonics + positive AM envelope)
fn generate_speech_like(base_freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let h1 = (2.0 * PI * base_freq * t).sin();
            let h2 = 0.6 * (2.0 * PI * base_freq * 2.0 * t).sin();
            let h3 = 0.4 * (2.0 * PI * base_freq * 3.0 * t).sin();
            let h4 = 0.25 * (2.0 * PI * base_freq * 4.0 * t).sin();
            // Positive envelope (0 to 1)
            let envelope = 0.5 + 0.5 * (2.0 * PI * 4.0 * t).sin();
            envelope * (h1 + h2 + h3 + h4) * amplitude
        })
        .collect()
}

/// Add delayed echo to mic signal
fn add_echo(mic: &[f32], ref_signal: &[f32], delay_samples: usize, attenuation: f32) -> Vec<f32> {
    let mut result = mic.to_vec();
    for i in delay_samples..mic.len() {
        if (i - delay_samples) < ref_signal.len() {
            result[i] += ref_signal[i - delay_samples] * attenuation;
        }
    }
    result
}

/// Calculate correlation coefficient
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

fn run_aec(mic_in: &[f32], far_end: &[f32], delay_ms: u16) -> Result<Vec<f32>> {
    let processor = Processor::new(SAMPLE_RATE)?;
    let mut config = Config::default();
    config.echo_canceller = Some(EchoCanceller::Full {
        stream_delay_ms: Some(delay_ms),
    });
    processor.set_config(config);

    let num_frames = mic_in.len() / FRAME_SIZE;
    let mut aec_out = Vec::with_capacity(mic_in.len());

    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;
        if end > far_end.len() { break; }

        // Convert to planar format: Vec<Vec<f32>> with 1 channel
        let mut ref_frame: Vec<Vec<f32>> = vec![far_end[start..end].to_vec()];
        let mut mic_frame: Vec<Vec<f32>> = vec![mic_in[start..end].to_vec()];

        // Process render first (speaker signal, immutable)
        processor.process_render_frame(&mut ref_frame)?;

        // Process capture (mic signal, gets modified)
        processor.process_capture_frame(&mut mic_frame)?;

        aec_out.extend_from_slice(&mic_frame[0]);
    }

    Ok(aec_out)
}

fn analyze(name: &str, near_end: &[f32], aec_out: &[f32], mic_in: &[f32]) {
    let compare_len = near_end.len().min(aec_out.len());
    let skip = SAMPLE_RATE as usize;

    let full_corr = calculate_correlation(&near_end[..compare_len], &aec_out[..compare_len]);
    let full_aec_rms = calculate_rms(&aec_out[..compare_len]);
    let full_near_rms = calculate_rms(&near_end[..compare_len]);
    let full_mic_rms = calculate_rms(&mic_in[..compare_len]);

    let post_corr = if compare_len > skip {
        calculate_correlation(&near_end[skip..compare_len], &aec_out[skip..compare_len])
    } else { 0.0 };
    let post_aec_rms = if compare_len > skip { calculate_rms(&aec_out[skip..compare_len]) } else { 0.0 };
    let post_near_rms = if compare_len > skip { calculate_rms(&near_end[skip..compare_len]) } else { 0.0 };
    let post_mic_rms = if compare_len > skip { calculate_rms(&mic_in[skip..compare_len]) } else { 0.0 };

    println!("\n=== {} ===", name);
    println!("  Full signal:");
    println!("    Mic  RMS: {:.4} -> AEC RMS: {:.4} ({:+.2} dB)", 
        full_mic_rms, full_aec_rms, 20.0 * (full_aec_rms / full_mic_rms.max(0.0001)).log10());
    println!("    Near RMS: {:.4}, Correlation: {:.4}", full_near_rms, full_corr);
    println!("  After 1s convergence:");
    println!("    Mic  RMS: {:.4} -> AEC RMS: {:.4} ({:+.2} dB)",
        post_mic_rms, post_aec_rms, 20.0 * (post_aec_rms / post_mic_rms.max(0.0001)).log10());
    println!("    Near RMS: {:.4}, Correlation: {:.4}", post_near_rms, post_corr);

    if post_corr > 0.9 {
        println!("  ✓✓ EXCELLENT");
    } else if post_corr > 0.7 {
        println!("  ✓ GOOD");
    } else if post_corr > 0.4 {
        println!("  ~ MODERATE");
    } else {
        println!("  ✗ POOR");
    }
}

fn main() -> Result<()> {
    let duration_secs = 5.0;
    let total_samples = (SAMPLE_RATE as f32 * duration_secs) as usize;

    println!("=== WebRTC AEC3 Test (Correct Usage) ===");
    println!("Sample rate: {} Hz, Frame size: {} samples", SAMPLE_RATE, FRAME_SIZE);
    println!("Duration: {:.1}s", duration_secs);

    // Test scenarios - start with pure tones (simple, predictable)
    let test_scenarios = vec![
        ("Test 1: 250Hz tone only, ref=silence",
            generate_sine(250.0, SAMPLE_RATE, total_samples, 0.5),
            vec![0.0; total_samples],
            0, 0.0),
        ("Test 2: 250Hz tone + 1000Hz echo 100ms @ -10dB",
            generate_sine(250.0, SAMPLE_RATE, total_samples, 0.5),
            generate_sine(1000.0, SAMPLE_RATE, total_samples, 0.5),
            100, 0.316),
        ("Test 3: 250Hz tone + 800Hz echo 200ms @ -15dB",
            generate_sine(250.0, SAMPLE_RATE, total_samples, 0.5),
            generate_sine(800.0, SAMPLE_RATE, total_samples, 0.5),
            200, 0.178),
    ];

    for (name, near_end, far_end, delay_ms, echo_atten) in test_scenarios {
        let delay_samples = delay_ms * SAMPLE_RATE as usize / 1000;
        let mic_in = add_echo(&near_end, &far_end, delay_samples, echo_atten);

        // Use the same delay for AEC estimation
        let aec_out = run_aec(&mic_in, &far_end, delay_ms as u16)?;
        analyze(name, &near_end, &aec_out, &mic_in);
    }

    // Test 4: Only echo, no near-end - try multiple delay values
    println!("\n=== Test 4: Only echo (no near-end) - Multiple delay values ===");
    {
        let far_end = generate_sine(1000.0, SAMPLE_RATE, total_samples, 0.5);
        let silence = vec![0.0; total_samples];
        let delay_samples = 100 * SAMPLE_RATE as usize / 1000;
        let mic4 = add_echo(&silence, &far_end, delay_samples, 0.316);
        let echo_only = add_echo(&silence, &far_end, delay_samples, 0.316);
        let echo4_rms = calculate_rms(&echo_only);

        for delay_ms in [0u16, 20u16, 50u16, 100u16, 200u16] {
            let aec_out4 = run_aec(&mic4, &far_end, delay_ms)?;
            let aec4_rms = calculate_rms(&aec_out4);
            println!("  delay_ms={:3}: Echo RMS {:.4}, AEC output RMS {:.4}, suppression {:+.2} dB",
                delay_ms, echo4_rms, aec4_rms, 20.0 * (aec4_rms / echo4_rms.max(0.0001)).log10());
        }
    }

    Ok(())
}
