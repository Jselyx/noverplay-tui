pub(super) fn convert_audio(
    input: &[f32],
    input_channels: usize,
    input_rate: u32,
    output_channels: usize,
    output_rate: u32,
) -> Vec<f32> {
    if input.is_empty() || input_channels == 0 || output_channels == 0 || input_rate == 0 {
        return Vec::new();
    }
    let input_frames = input.len() / input_channels;
    let output_frames =
        ((input_frames as u64 * u64::from(output_rate)) / u64::from(input_rate)).max(1) as usize;
    let mut output = Vec::with_capacity(output_frames * output_channels);

    // Линейная интерполяция тут не Нобелевка, зато не тащит DSP-комбайн весом с тюремный барак 🤡
    for output_frame in 0..output_frames {
        let position = output_frame as f64 * input_rate as f64 / output_rate as f64;
        let left = (position.floor() as usize).min(input_frames - 1);
        let right = (left + 1).min(input_frames - 1);
        let fraction = (position - left as f64) as f32;
        for output_channel in 0..output_channels {
            let sample = if input_channels == 1 {
                input[left] * (1.0 - fraction) + input[right] * fraction
            } else if output_channels == 1 {
                let left_average = average_frame(input, left, input_channels);
                let right_average = average_frame(input, right, input_channels);
                left_average * (1.0 - fraction) + right_average * fraction
            } else {
                let input_channel = output_channel.min(input_channels - 1);
                let left_sample = input[left * input_channels + input_channel];
                let right_sample = input[right * input_channels + input_channel];
                left_sample * (1.0 - fraction) + right_sample * fraction
            };
            output.push(sample);
        }
    }
    output
}

fn average_frame(input: &[f32], frame: usize, channels: usize) -> f32 {
    let start = frame * channels;
    input[start..start + channels].iter().sum::<f32>() / channels as f32
}
