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
    let duration_secs = 5.0;
    let total_samples = (SAMPLE_RATE as f32 * duration_secs) as usize;

    println!("=== Minimal AEC Test: Pure Tones ===");

    // Test A: 1000Hz sine, no echo
    println!("\n--- Test A: 1000Hz mic only, ref=silence ---");
    let mic_a = generate_sine(1000.0, SAMPLE_RATE, total_samples, 0.5);
    let ref_a = vec![0.0; total_samples];

    let processor = Processor::new(SAMPLE_RATE)?;
    let mut config = Config::default();
    config.echo_canceller = Some(EchoCanceller::Full {
        stream_delay_ms: None,
    });
    processor.set_config(config);

    let num_frames = mic_a.len() / FRAME_SIZE;
    let mut out_a = Vec::with_capacity(mic_a.len());

    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;

        let ref_frame = ref_a[start..end].to_vec();
        let mic_frame = mic_a[start..end].to_vec();

        let mut ref_ch = vec![ref_frame];
        let mut mic_ch = vec![mic_frame];

        processor.process_render_frame(&mut ref_ch)?;
        processor.process_capture_frame(&mut mic_ch)?;

        out_a.extend_from_slice(&mic_ch[0]);
    }

    // Check correlation after convergence (skip first 1s)
    let skip = SAMPLE_RATE as usize;
    let corr_a = calculate_correlation(&mic_a[skip..], &out_a[skip..]);
    println!("  Correlation (mic vs AEC output, after 1s): {:.4}", corr_a);

    // Print first few samples to see what's happening
    println!("  First 10 samples of mic: {:?}", &mic_a[..10].iter().map(|x| (x * 1000.0) as i32).collect::<Vec<_>>());
    println!("  First 10 samples of AEC: {:?}", &out_a[..10].iter().map(|x| (x * 1000.0) as i32).collect::<Vec<_>>());

    // Test B: 1000Hz mic + 1000Hz echo
    println!("\n--- Test B: 1000Hz mic + 1000Hz echo 100ms ---");
    let ref_b = generate_sine(1000.0, SAMPLE_RATE, total_samples, 0.5);
    let mut mic_b = mic_a.clone();
    let delay_samples = 100 * SAMPLE_RATE as usize / 1000; // 100ms
    for i in delay_samples..mic_b.len() {
        if (i - delay_samples) < ref_b.len() {
            mic_b[i] += ref_b[i - delay_samples] * 0.5;
        }
    }

    let processor2 = Processor::new(SAMPLE_RATE)?;
    let mut config2 = Config::default();
    config2.echo_canceller = Some(EchoCanceller::Full {
        stream_delay_ms: None,
    });
    processor2.set_config(config2);

    let mut out_b = Vec::with_capacity(mic_b.len());
    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;

        let ref_frame = ref_b[start..end].to_vec();
        let mic_frame = mic_b[start..end].to_vec();

        let mut ref_ch = vec![ref_frame];
        let mut mic_ch = vec![mic_frame];

        processor2.process_render_frame(&mut ref_ch)?;
        processor2.process_capture_frame(&mut mic_ch)?;

        out_b.extend_from_slice(&mic_ch[0]);
    }

    let corr_b = calculate_correlation(&mic_a[skip..], &out_b[skip..]);
    println!("  Correlation (original mic vs AEC output, after 1s): {:.4}", corr_b);

    Ok(())
}
