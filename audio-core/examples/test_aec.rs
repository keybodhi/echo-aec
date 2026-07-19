use anyhow::Result;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing_config::{Config, EchoCanceller, NoiseSuppression, NoiseSuppressionLevel};
use std::f32::consts::PI;

fn generate_sine(freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * PI * freq * t).sin() * amplitude
        })
        .collect()
}

/// Generate a more speech-like signal with multiple frequencies and amplitude modulation
fn generate_speech_like(base_freq: f32, sample_rate: u32, num_samples: usize, amplitude: f32) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Fundamental + harmonics with amplitude modulation
            let f0 = base_freq;
            let f1 = base_freq * 2.0;
            let f2 = base_freq * 3.0;
            let f3 = base_freq * 4.0;
            // Amplitude modulation (simulating speech envelope)
            let envelope = 0.5 + 0.5 * (2.0 * PI * 4.0 * t).sin();
            let harmonic1 = (2.0 * PI * f0 * t).sin();
            let harmonic2 = 0.5 * (2.0 * PI * f1 * t).sin();
            let harmonic3 = 0.3 * (2.0 * PI * f2 * t).sin();
            let harmonic4 = 0.2 * (2.0 * PI * f3 * t).sin();
            // Add some noise to make it more realistic
            let noise = (rand_simple(t * 12345.6789) - 0.5) * 0.1;
            (harmonic1 + harmonic2 + harmonic3 + harmonic4 + noise) * amplitude * envelope
        })
        .collect()
}

/// Simple pseudo-random function
fn rand_simple(x: f32) -> f32 {
    let x = x.fract() * 43758.5453;
    (x.sin() + 1.0) / 2.0
}

fn calculate_rms(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Calculate energy of a specific frequency band using a simple DFT
fn calculate_band_energy(samples: &[f32], freq: f32, sample_rate: u32) -> f32 {
    let mut sum = 0.0;
    let num_samples = samples.len();
    for (i, &sample) in samples.iter().enumerate() {
        let t = i as f32 / sample_rate as f32;
        // Use dot product with sine and cosine
        let phase = 2.0 * PI * freq * t;
        sum += sample * phase.sin() / (num_samples as f32 / 2.0);
    }
    sum.abs()
}

fn main() -> Result<()> {
    let sample_rate = 48000u32;
    let frame_size = 480usize; // 10ms at 48kHz
    let duration_secs = 5.0;  // Long enough to verify convergence
    let total_samples = (sample_rate as f32 * duration_secs) as usize;
    
    println!("=== AEC Test - Frequency Band Analysis ===");
    println!("Sample rate: {} Hz", sample_rate);
    println!("Frame size: {} samples (10ms)", frame_size);
    println!("Duration: {} seconds", duration_secs);
    println!();
    
    // Test 1: Only far-end signal (no near-end)
    // This should be completely removed by AEC
    println!("--- Test 1: Only far-end signal (speech-like) ---");
    {
        // Use speech-like signal for better delay estimation
        let far_end = generate_speech_like(200.0, sample_rate, total_samples, 0.5);
        // Mic only picks up the echo (delayed and attenuated far-end)
        let delay_samples = 100;
        let echo_attenuation = 0.3f32;
        let mut mic_signal = vec![0.0f32; total_samples];
        for i in delay_samples..total_samples {
            mic_signal[i] = far_end[i - delay_samples] * echo_attenuation;
        }
        
        let processor = Processor::new(sample_rate)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        // Disable noise suppression to test AEC alone
        config.noise_suppression = Some(NoiseSuppression {
            level: NoiseSuppressionLevel::Low,
            analyze_linear_aec_output: false,
        });
        processor.set_config(config);
        
        let num_frames = total_samples / frame_size;
        let mut output = Vec::with_capacity(total_samples);
        
        for frame_idx in 0..num_frames {
            let start = frame_idx * frame_size;
            let end = start + frame_size;
            
            let ref_frame = far_end[start..end].to_vec();
            let mut mic_frame = mic_signal[start..end].to_vec();
            
            let mut ref_ch = vec![ref_frame];
            processor.process_render_frame(&mut ref_ch)?;
            
            let mut mic_ch = vec![mic_frame];
            processor.process_capture_frame(&mut mic_ch)?;
            
            output.extend_from_slice(&mic_ch[0]);
        }
        
        // Analyze convergence over time
        println!("  Convergence analysis (1kHz echo suppression over time):");
        let segments = 15;
        let segment_size = total_samples / segments;
        for i in 0..segments {
            let start = i * segment_size;
            let end = start + segment_size;
            if end > output.len() { break; }
            let input_seg = if end < mic_signal.len() {
                &mic_signal[start..end]
            } else {
                &mic_signal[start..]
            };
            let output_seg = &output[start..end.min(output.len())];
            let input_rms = calculate_rms(input_seg);
            let output_rms = calculate_rms(output_seg);
            let time = i as f32 * duration_secs / segments as f32;
            let supp = if input_rms > 0.0 && output_rms > 0.0 {
                20.0 * (output_rms / input_rms).log10()
            } else {
                -100.0
            };
            println!("  t={:5.1}s: suppression = {:7.2} dB", time, supp);
        }
        println!();
    }
    
    // Test 2: Only near-end signal (no echo)
    // This should be preserved by AEC
    println!("--- Test 2: Only near-end signal, no echo ---");
    {
        let near_end = generate_speech_like(300.0, sample_rate, total_samples, 0.5);
        let silence = vec![0.0f32; total_samples];
        
        let processor = Processor::new(sample_rate)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        // Disable noise suppression
        config.noise_suppression = None;
        processor.set_config(config);
        
        let num_frames = total_samples / frame_size;
        let mut output = Vec::with_capacity(total_samples);
        
        for frame_idx in 0..num_frames {
            let start = frame_idx * frame_size;
            let end = start + frame_size;
            
            let ref_frame = silence[start..end].to_vec();
            let mut mic_frame = near_end[start..end].to_vec();
            
            let mut ref_ch = vec![ref_frame];
            processor.process_render_frame(&mut ref_ch)?;
            
            let mut mic_ch = vec![mic_frame];
            processor.process_capture_frame(&mut mic_ch)?;
            
            output.extend_from_slice(&mic_ch[0]);
        }
        
        let input_rms = calculate_rms(&near_end);
        let output_rms = calculate_rms(&output);
        let input_500hz = calculate_band_energy(&near_end, 500.0, sample_rate);
        let output_500hz = calculate_band_energy(&output, 500.0, sample_rate);
        
        println!("  Input  RMS:  {:.6}, 500Hz energy: {:.6}", input_rms, input_500hz);
        println!("  Output RMS:  {:.6}, 500Hz energy: {:.6}", output_rms, output_500hz);
        println!("  Total change:    {:.2} dB", 20.0 * (output_rms / input_rms).log10());
        println!("  500Hz change:    {:.2} dB", 20.0 * (output_500hz / input_500hz).log10());
        println!();
    }
    
    // Test 3: Both far-end and near-end (double-talk)
    // This is the most important test
    println!("--- Test 3: Double-talk ---");
    {
        let far_end = generate_speech_like(200.0, sample_rate, total_samples, 0.5);
        let near_end = generate_speech_like(300.0, sample_rate, total_samples, 0.5);
        
        // Mic picks up echo + near-end
        let delay_samples = 100;
        let echo_attenuation = 0.3f32;
        let mut mic_signal: Vec<f32> = near_end.iter()
            .enumerate()
            .map(|(i, &n)| {
                if i >= delay_samples {
                    n + far_end[i - delay_samples] * echo_attenuation
                } else {
                    n
                }
            })
            .collect();
        
        let processor = Processor::new(sample_rate)?;
        let mut config = Config::default();
        config.echo_canceller = Some(EchoCanceller::Full {
            stream_delay_ms: None,
        });
        // Disable noise suppression
        config.noise_suppression = None;
        processor.set_config(config);
        
        let num_frames = total_samples / frame_size;
        let mut output = Vec::with_capacity(total_samples);
        
        for frame_idx in 0..num_frames {
            let start = frame_idx * frame_size;
            let end = start + frame_size;
            
            let ref_frame = far_end[start..end].to_vec();
            let mut mic_frame = mic_signal[start..end].to_vec();
            
            let mut ref_ch = vec![ref_frame];
            processor.process_render_frame(&mut ref_ch)?;
            
            let mut mic_ch = vec![mic_frame];
            processor.process_capture_frame(&mut mic_ch)?;
            
            output.extend_from_slice(&mic_ch[0]);
        }
        
        // Calculate energy in low band (near-end, ~300Hz fundamental)
        let input_low = calculate_band_energy(&mic_signal[100..], 300.0, sample_rate);
        let output_low = calculate_band_energy(&output[100..], 300.0, sample_rate);
        // Calculate energy in high band (echo, ~200Hz fundamental but with higher harmonics)
        let input_high = calculate_band_energy(&mic_signal[100..], 800.0, sample_rate);
        let output_high = calculate_band_energy(&output[100..], 800.0, sample_rate);
        
        println!("  Input  300Hz (near-end):  {:.6}", input_low);
        println!("  Output 300Hz (near-end):  {:.6}", output_low);
        println!("  300Hz retention:  {:.2} dB", 20.0 * (output_low / input_low).log10());
        println!();
        println!("  Input  800Hz (echo):  {:.6}", input_high);
        println!("  Output 800Hz (echo):  {:.6}", output_high);
        println!("  800Hz suppression:  {:.2} dB", 20.0 * (output_high / input_high).log10());
        println!();
        
        println!("  Expected: 300Hz preserved, 800Hz removed");
        println!("  If 300Hz is also suppressed, AEC is too aggressive");
        println!("  If 800Hz is not suppressed, AEC is not working");
    }
    
    // Save test 3 signals
    let far_end = generate_speech_like(200.0, sample_rate, total_samples, 0.5);
    let near_end = generate_speech_like(300.0, sample_rate, total_samples, 0.5);
    let delay_samples = 100;
    let echo_attenuation = 0.3f32;
    let mic_signal: Vec<f32> = near_end.iter()
        .enumerate()
        .map(|(i, &n)| {
            if i >= delay_samples {
                n + far_end[i - delay_samples] * echo_attenuation
            } else {
                n
            }
        })
        .collect();
    
    save_wav("test_input.wav", &mic_signal, sample_rate)?;
    save_wav("test_nearend.wav", &near_end, sample_rate)?;
    save_wav("test_farend.wav", &far_end, sample_rate)?;
    println!();
    println!("Saved WAV files for manual verification");
    
    Ok(())
}

fn save_wav(filename: &str, samples: &[f32], sample_rate: u32) -> Result<()> {
    use std::fs::File;
    use std::io::Write;
    
    let mut file = File::create(filename)?;
    
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * 2;
    let block_align = 2u16;
    let bits_per_sample = 16u16;
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
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;
    
    for &sample in samples {
        let clamped = sample.max(-1.0).min(1.0);
        let i16_sample = (clamped * 32767.0) as i16;
        file.write_all(&i16_sample.to_le_bytes())?;
    }
    
    Ok(())
}
