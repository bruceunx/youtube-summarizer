use ffmpeg_next::{self as ffmpeg};
use hound::{WavReader, WavWriter};
use std::error::Error;
use std::fs;
use std::path::Path;

pub struct AudioSplitter {
    chunk_duration: i64,
}

pub struct WavSplitter {
    chunk_duration: u32,
}

impl AudioSplitter {
    pub fn new(duration_seconds: i64) -> Self {
        Self {
            chunk_duration: duration_seconds,
        }
    }

    pub fn split(&self, input_file: &Path, output_dir: &Path) -> Result<(), ffmpeg::Error> {
        ffmpeg::init()?;
        if !output_dir.is_dir() {
            std::fs::create_dir_all(output_dir).map_err(|_| ffmpeg::Error::Other { errno: -1 })?;
        }
        let file_suffix = input_file.extension().unwrap().to_str().unwrap();

        let mut input_ctx = ffmpeg::format::input(input_file)?;
        let audio_stream = input_ctx
            .streams()
            .best(ffmpeg::media::Type::Audio)
            .ok_or(ffmpeg::Error::StreamNotFound)?;

        let audio_parameters = audio_stream.parameters();
        let codec =
            ffmpeg::codec::context::Context::from_parameters(audio_parameters.clone()).unwrap();

        let total_duration = input_ctx.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE);
        let num_chunks = (total_duration / self.chunk_duration as f64).ceil() as i64;

        let input_time_base = audio_stream.time_base();
        for chunk_index in 0..num_chunks {
            let start_time = chunk_index * self.chunk_duration;
            let output_filename = format!("chunk_{:03}.{}", chunk_index + 1, file_suffix);
            let output_path = output_dir.join(output_filename);
            let mut output_ctx = ffmpeg::format::output(&output_path)?;

            let mut output_stream = output_ctx.add_stream(codec.codec())?;
            output_stream.set_parameters(audio_parameters.clone());
            output_stream.set_time_base(input_time_base);

            let start_ts = start_time * ffmpeg::ffi::AV_TIME_BASE as i64;

            // let start_ts = (start_time as f64 * input_time_base.denominator() as f64
            //     / input_time_base.numerator() as f64) as i64;
            let end_time = ((start_time + self.chunk_duration) * ffmpeg::ffi::AV_TIME_BASE as i64)
                .min(input_ctx.duration());

            input_ctx.seek(start_ts, ..start_ts)?;

            output_ctx.set_metadata(input_ctx.metadata().to_owned());
            output_ctx.write_header()?;

            let mut last_pts = 0;
            let mut pts_offset = 0;
            let mut first_pts = None;
            for (stream, packet) in input_ctx.packets() {
                let pts = packet.pts().unwrap_or(0);
                let current_time = pts * ffmpeg::ffi::AV_TIME_BASE as i64
                    / stream.time_base().denominator() as i64;

                if current_time >= end_time {
                    break;
                }
                let mut new_packet = packet.clone();
                new_packet.set_position(-1);
                new_packet.set_stream(0);

                if let Some(first_pts_value) = first_pts {
                    let adjusted_pts = pts - first_pts_value;

                    if adjusted_pts < last_pts {
                        pts_offset = last_pts;
                    }

                    let final_pts = adjusted_pts + pts_offset;
                    new_packet.set_pts(Some(final_pts));
                    new_packet.set_dts(Some(final_pts));

                    last_pts = final_pts;
                } else {
                    first_pts = Some(pts);
                    new_packet.set_pts(Some(0));
                    new_packet.set_dts(Some(0));
                }

                new_packet.write_interleaved(&mut output_ctx)?;
            }
            output_ctx.write_trailer()?;
        }
        Ok(())
    }
}

impl WavSplitter {
    pub fn new(duration: u32) -> Self {
        Self {
            chunk_duration: duration,
        }
    }

    pub fn split_wav(&self, input_file: &Path, output_dir: &Path) -> Result<(), Box<dyn Error>> {
        let reader = WavReader::open(input_file)?;
        let spec = reader.spec();
        let samples: Vec<i16> = reader.into_samples::<i16>().collect::<Result<_, _>>()?;

        let sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;
        let chunk_size = (self.chunk_duration * sample_rate * channels as u32) as usize;

        if !output_dir.is_dir() {
            fs::create_dir_all(output_dir)?;
        }

        for (i, chunk) in samples.chunks(chunk_size).enumerate() {
            let output_filename = format!("chunk_{:03}.wav", i + 1);
            let output_file = output_dir.join(output_filename);
            let mut writer = WavWriter::create(&output_file, spec)?;
            for &sample in chunk {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use super::*;

    #[test]
    fn split_audio_works() {
        let audio_splitter = AudioSplitter::new(60 * 10);
        // let input_file = PathBuf::from_str("./sample.webm").unwrap();
        let input_file = PathBuf::from_str("./sample.wav").unwrap();
        let output_dir = PathBuf::from_str("output_dir").unwrap();
        let result = audio_splitter.split(&input_file, &output_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn split_wav_works() {
        let wav_splitter = WavSplitter::new(300);
        let input_file = PathBuf::from_str("./sample.wav").unwrap();
        let output_dir = PathBuf::from_str("output_dir").unwrap();
        let result = wav_splitter.split_wav(&input_file, &output_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ffmpeg() {
        ffmpeg::init().unwrap();

        let input_file = PathBuf::from_str("./sample.m4a").expect("failed to find the file");
        match ffmpeg::format::input(&input_file) {
            Ok(context) => {
                let stream = context.streams().best(ffmpeg::media::Type::Audio).unwrap();

                let total_duration =
                    context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE);

                println!("stream index {}:", stream.index());
                println!("\ttime_base: {}", stream.time_base());
                println!("\tstart_time: {}", stream.start_time());
                println!("\tduration (stream timebase): {}", stream.duration());
                println!(
                    "\tduration (seconds): {:.2}",
                    stream.duration() as f64 * f64::from(stream.time_base())
                );
                println!("\ttotal duration {:.2}", total_duration);
                println!("\tframes: {}", stream.frames());
                println!("\tdisposition: {:?}", stream.disposition());
                println!("\tdiscard: {:?}", stream.discard());
                println!("\trate: {}", stream.rate());

                let codec =
                    ffmpeg::codec::context::Context::from_parameters(stream.parameters()).unwrap();
                println!("\tmedium: {:?}", codec.medium());
                println!("\tid: {:?}", codec.id());

                if let Ok(audio) = codec.decoder().audio() {
                    println!("\tbit_rate: {}", audio.bit_rate());
                    println!("\tmax_rate: {}", audio.max_bit_rate());
                    println!("\tdelay: {}", audio.delay());
                    println!("\taudio.rate: {}", audio.rate());
                    println!("\taudio.channels: {}", audio.channels());
                    println!("\taudio.format: {:?}", audio.format());
                    println!("\taudio.frames: {}", audio.frames());
                    println!("\taudio.align: {}", audio.align());
                    println!("\taudio.channel_layout: {:?}", audio.channel_layout());
                }

                if let Some(stream) = context.streams().best(ffmpeg::media::Type::Audio) {
                    println!("Best subtitle stream index: {}", stream.index());
                } else {
                    println!("error to find the best stream");
                };
                println!(
                    "duration (seconds): {:.2}",
                    context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
                );
            }
            Err(e) => eprintln!("cannot get content {}", e),
        };

        // assert_eq!(1, 2);
    }
}
