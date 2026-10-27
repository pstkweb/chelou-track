// Décodage d'un backing track entier (wav/mp3/flac) en stéréo f32 entrelacé, puis
// rééchantillonnage à la fréquence de la sortie (ARCHITECTURE.md §4 : chargé avant lecture).

use anyhow::{anyhow, Context, Result};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Audio décodé, **stéréo entrelacé**.
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// Canal gauche seul — entrée de la détection du silence.
    pub fn left(&self) -> Vec<f32> {
        self.samples.iter().step_by(2).copied().collect()
    }

    pub fn duration_ms(&self) -> f64 {
        self.frames() as f64 * 1000.0 / self.sample_rate as f64
    }
}

/// Décode un fichier entier. Mono → dupliqué sur les 2 canaux ; > 2 canaux → les 2 premiers.
pub fn decode_stereo(bytes: Vec<u8>, ext: Option<&str>) -> Result<Pcm> {
    let mut hint = Hint::new();
    if let Some(ext) = ext {
        hint.with_extension(ext);
    }
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("format audio non reconnu")?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("aucune piste audio"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| anyhow!("paramètres de codec absents"))?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .context("codec audio non supporté")?;

    let mut samples = Vec::new();
    let mut packet_buf: Vec<f32> = Vec::new();
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    while let Some(packet) = format.next_packet().context("lecture du fichier audio")? {
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(e) if is_skippable(&e) => continue,
            Err(e) => return Err(e).context("décodage audio"),
        };
        let channels = decoded.spec().channels().count();
        sample_rate = decoded.spec().rate();
        packet_buf.clear();
        decoded.copy_to_vec_interleaved(&mut packet_buf);
        for frame in packet_buf.chunks_exact(channels) {
            let left = frame[0];
            let right = if channels > 1 { frame[1] } else { left };
            samples.push(left);
            samples.push(right);
        }
    }
    if sample_rate == 0 {
        return Err(anyhow!("fréquence d'échantillonnage inconnue"));
    }
    Ok(Pcm {
        samples,
        sample_rate,
    })
}

/// Paquet corrompu (fréquent dans les mp3 réels, toléré par les lecteurs) : on le saute au lieu
/// de faire échouer tout le chargement. Les erreurs d'E/S ou de format restent fatales.
fn is_skippable(e: &symphonia::core::errors::Error) -> bool {
    matches!(e, symphonia::core::errors::Error::DecodeError(_))
}

/// Rééchantillonne à `target_rate` en conservant la durée (retard du filtre retiré).
pub fn resample(pcm: &Pcm, target_rate: u32) -> Result<Pcm> {
    if pcm.sample_rate == target_rate || pcm.samples.is_empty() {
        return Ok(Pcm {
            samples: pcm.samples.clone(),
            sample_rate: target_rate,
        });
    }
    let frames = pcm.frames();
    let input = InterleavedSlice::new(&pcm.samples[..frames * 2], 2, frames)
        .map_err(|e| anyhow!("tampon audio invalide : {e:?}"))?;
    let mut resampler = Fft::<f32>::new(
        pcm.sample_rate as usize,
        target_rate as usize,
        1024,
        2,
        FixedSync::Both,
    )
    .map_err(|e| anyhow!("rééchantillonneur : {e}"))?;
    let out = resampler
        .process_all(&input, frames, None)
        .map_err(|e| anyhow!("rééchantillonnage : {e}"))?;
    Ok(Pcm {
        samples: out.take_data(),
        sample_rate: target_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// En-tête RIFF/PCM 16 bits écrit à la main : `frames` est entrelacé.
    fn wav_i16(channels: u16, rate: u32, frames: &[i16]) -> Vec<u8> {
        let data_len = (frames.len() * 2) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for s in frames {
            b.extend_from_slice(&s.to_le_bytes());
        }
        b
    }

    fn sine(rate: u32, seconds: f64) -> Vec<f32> {
        let n = (rate as f64 * seconds) as usize;
        (0..n)
            .flat_map(|i| {
                let v = (i as f64 / rate as f64 * 440.0 * std::f64::consts::TAU).sin() as f32 * 0.5;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn decode_mono_wav_duplicates_channel() {
        let frames: Vec<i16> = (0..1000).map(|i| (i * 20) as i16).collect();
        let pcm = decode_stereo(wav_i16(1, 44100, &frames), Some("wav")).unwrap();
        assert_eq!(pcm.samples.len(), 2000);
        assert_eq!(pcm.samples[10], pcm.samples[11]);
        assert!(pcm.samples[10] != 0.0);
    }

    #[test]
    fn decode_stereo_wav_keeps_channels() {
        let frames: Vec<i16> = (0..100).flat_map(|_| [16384i16, -16384]).collect();
        let pcm = decode_stereo(wav_i16(2, 44100, &frames), Some("wav")).unwrap();
        assert!((pcm.samples[0] - 0.5).abs() < 1e-3);
        assert!((pcm.samples[1] + 0.5).abs() < 1e-3);
    }

    #[test]
    fn decode_reports_source_rate() {
        let pcm = decode_stereo(wav_i16(1, 44100, &[0; 10]), Some("wav")).unwrap();
        assert_eq!(pcm.sample_rate, 44100);
    }

    #[test]
    fn decode_garbage_is_error() {
        assert!(decode_stereo(vec![1, 2, 3], Some("wav")).is_err());
    }

    #[test]
    fn corrupt_packet_is_skipped_but_io_failure_is_not() {
        use symphonia::core::errors::Error;
        assert!(is_skippable(&Error::DecodeError("trame mp3 invalide")));
        assert!(!is_skippable(&Error::IoError(std::io::Error::other(
            "coupure"
        ))));
        assert!(!is_skippable(&Error::Unsupported("codec")));
    }

    #[test]
    fn resample_preserves_duration() {
        let pcm = Pcm {
            samples: sine(44100, 1.0),
            sample_rate: 44100,
        };
        let before = pcm.duration_ms();
        let out = resample(&pcm, 48000).unwrap();
        assert_eq!(out.sample_rate, 48000);
        assert!(
            (out.duration_ms() - before).abs() <= 1.0,
            "{} vs {before}",
            out.duration_ms()
        );
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let samples = sine(48000, 0.1);
        let out = resample(
            &Pcm {
                samples: samples.clone(),
                sample_rate: 48000,
            },
            48000,
        )
        .unwrap();
        assert_eq!(out.samples, samples);
    }

    #[test]
    fn left_extracts_first_channel() {
        let pcm = Pcm {
            samples: vec![1.0, -1.0, 2.0, -2.0],
            sample_rate: 1000,
        };
        assert_eq!(pcm.left(), vec![1.0, 2.0]);
        assert_eq!(pcm.duration_ms(), 2.0);
    }
}
