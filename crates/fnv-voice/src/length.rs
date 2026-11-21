//! How long a sound file is, from its own headers (docs/spec-notes/voice.md §5). Nothing
//! is decoded: an Ogg Vorbis file gives its sample rate in the identification header and
//! its sample count as the last page's granule position; a WAV file gives both in its
//! `fmt ` and `data` chunks.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundFormat {
    OggVorbis,
    Wav,
}

/// What the headers of a sound file say.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SoundInfo {
    pub format: SoundFormat,
    pub channels: u16,
    pub sample_rate: u32,
    /// Sample frames (one per channel set).
    pub samples: u64,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LengthError {
    #[error("not an Ogg file (no OggS page at the start)")]
    NotOgg,
    #[error("the first Ogg packet is not a Vorbis identification header")]
    NotVorbis,
    #[error("not a RIFF WAVE file")]
    NotWav,
    #[error("the file ends inside its {0}")]
    Truncated(&'static str),
    #[error("no {0}")]
    Missing(&'static str),
    #[error("sample rate 0")]
    ZeroRate,
    #[error("unknown sound file type {0:?}")]
    UnknownType(String),
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64le(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// "No packet ends on this page" [documented: RFC 3533 §6, granule position -1].
const NO_GRANULE: u64 = u64::MAX;

/// One Ogg page [documented: RFC 3533 §6: capture pattern `OggS`, version, header type,
/// granule position (i64), serial number, sequence number, checksum, segment count, the
/// segment table, then the segments].
struct Page<'a> {
    granule: u64,
    serial: u32,
    /// The body up to the end of its first packet on this page (first lacing value below
    /// 255), which is all the identification header needs.
    first_packet: &'a [u8],
    next: usize,
}

fn page(b: &[u8], at: usize) -> Option<Page<'_>> {
    if b.get(at..at + 4)? != b"OggS" || *b.get(at + 4)? != 0 {
        return None;
    }
    let granule = u64le(b, at + 6)?;
    let serial = u32le(b, at + 14)?;
    let segments = *b.get(at + 26)? as usize;
    let table = b.get(at + 27..at + 27 + segments)?;
    let body_at = at + 27 + segments;
    let body_len: usize = table.iter().map(|&l| l as usize).sum();
    let body = b.get(body_at..body_at + body_len)?;
    let mut first = 0usize;
    for &l in table {
        first += l as usize;
        if l < 255 {
            break;
        }
    }
    Some(Page { granule, serial, first_packet: &body[..first], next: body_at + body_len })
}

/// Length of an Ogg Vorbis file.
///
/// The identification header is the first packet of the stream: packet type 1, the
/// letters `vorbis`, version (u32, 0), channel count (u8), sample rate (u32)
/// [documented: Vorbis I specification §4.2.2]. A page's granule position is the number
/// of samples of the stream decoded once the last packet ending on that page is
/// [documented: Vorbis I specification §A.2], so the last page that has one gives the
/// sample count. A file cut short keeps the last whole page's count.
pub fn ogg_vorbis_info(bytes: &[u8]) -> Result<SoundInfo, LengthError> {
    let first = page(bytes, 0).ok_or(LengthError::NotOgg)?;
    let id = first.first_packet;
    if id.len() < 16 || id[0] != 1 || &id[1..7] != b"vorbis" || u32le(id, 7) != Some(0) {
        return Err(LengthError::NotVorbis);
    }
    let channels = id[11] as u16;
    let sample_rate = u32le(id, 12).ok_or(LengthError::NotVorbis)?;
    if sample_rate == 0 {
        return Err(LengthError::ZeroRate);
    }
    let mut samples = None;
    let mut at = first.next;
    while let Some(p) = page(bytes, at) {
        if p.serial == first.serial && p.granule != NO_GRANULE {
            samples = Some(p.granule);
        }
        at = p.next;
    }
    let samples = samples.ok_or(LengthError::Missing("page with a sample position after the header"))?;
    Ok(SoundInfo { format: SoundFormat::OggVorbis, channels, sample_rate, samples, seconds: samples as f64 / sample_rate as f64 })
}

/// Length of a RIFF WAVE file.
///
/// Chunks of a four-letter tag, a u32 size and the data, padded to an even length; the
/// `fmt ` chunk holds format tag, channels, sample rate, average bytes per second, block
/// align and bits per sample; the `data` chunk holds the samples [documented: Microsoft
/// "Multimedia Programming Interface and Data Specifications 1.0", WAVE form]. PCM
/// (format 1), float (3) and extensible (0xFFFE): `data` size / block align sample
/// frames. Other formats: the `fact` chunk's sample count when present, else `data`
/// size / average bytes per second.
pub fn wav_info(bytes: &[u8]) -> Result<SoundInfo, LengthError> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(LengthError::NotWav);
    }
    let mut at = 12usize;
    let mut fmt: Option<(u16, u16, u32, u32, u16)> = None;
    let mut fact: Option<u32> = None;
    let mut data: Option<u64> = None;
    while let (Some(tag), Some(size)) = (bytes.get(at..at + 4), u32le(bytes, at + 4)) {
        let body = at + 8;
        let size = size as usize;
        match tag {
            b"fmt " => {
                let f = (|| Some((u16le(bytes, body)?, u16le(bytes, body + 2)?, u32le(bytes, body + 4)?, u32le(bytes, body + 8)?, u16le(bytes, body + 12)?)))();
                fmt = Some(f.filter(|_| size >= 14).ok_or(LengthError::Truncated("fmt chunk"))?);
            }
            b"fact" => fact = u32le(bytes, body).filter(|_| size >= 4),
            b"data" => {
                // A data chunk that claims more than the file holds counts what is there.
                data = Some(size.min(bytes.len().saturating_sub(body)) as u64);
                break;
            }
            _ => {}
        }
        at = body + size + (size & 1);
    }
    let (format, channels, sample_rate, byte_rate, block_align) = fmt.ok_or(LengthError::Missing("fmt chunk before the data"))?;
    let data = data.ok_or(LengthError::Missing("data chunk"))?;
    if sample_rate == 0 {
        return Err(LengthError::ZeroRate);
    }
    let pcm = matches!(format, 1 | 3 | 0xFFFE);
    let (samples, seconds) = if pcm && block_align > 0 {
        let n = data / block_align as u64;
        (n, n as f64 / sample_rate as f64)
    } else if let Some(n) = fact {
        (n as u64, n as f64 / sample_rate as f64)
    } else if byte_rate > 0 {
        let s = data as f64 / byte_rate as f64;
        ((s * sample_rate as f64).round() as u64, s)
    } else {
        return Err(LengthError::Missing("block align, fact chunk or byte rate to measure the data with"));
    };
    Ok(SoundInfo { format: SoundFormat::Wav, channels, sample_rate, samples, seconds })
}

/// Length of a sound by what the file starts with (`OggS` or `RIFF`).
pub fn sound_info(bytes: &[u8]) -> Result<SoundInfo, LengthError> {
    match bytes.get(0..4) {
        Some(b"OggS") => ogg_vorbis_info(bytes),
        Some(b"RIFF") => wav_info(bytes),
        other => Err(LengthError::UnknownType(String::from_utf8_lossy(other.unwrap_or_default()).into_owned())),
    }
}

/// Synthetic sound files for tests (this crate's and its users'): headers and page
/// structure only, no audio that would decode.
pub mod synth {
    /// One Ogg page holding `body` as one packet that ends on the page.
    pub fn ogg_page(header_type: u8, granule: u64, serial: u32, sequence: u32, body: &[u8]) -> Vec<u8> {
        let mut lacing = vec![255u8; body.len() / 255];
        lacing.push((body.len() % 255) as u8);
        let mut p = b"OggS".to_vec();
        p.push(0);
        p.push(header_type);
        p.extend(granule.to_le_bytes());
        p.extend(serial.to_le_bytes());
        p.extend(sequence.to_le_bytes());
        p.extend(0u32.to_le_bytes()); // checksum: not read by the length reader
        p.push(lacing.len() as u8);
        p.extend(lacing);
        p.extend(body);
        p
    }

    /// A Vorbis identification header packet.
    pub fn vorbis_id(channels: u8, sample_rate: u32) -> Vec<u8> {
        let mut h = vec![1u8];
        h.extend(b"vorbis");
        h.extend(0u32.to_le_bytes());
        h.push(channels);
        h.extend(sample_rate.to_le_bytes());
        h.extend([0u8; 12]); // bitrate maximum, nominal, minimum
        h.push(0xB8); // block sizes 256 and 2048
        h.push(1); // framing bit
        h
    }

    /// The page structure of an Ogg Vorbis file of `samples` sample frames: the
    /// identification page, a page standing for the other headers, and `audio_pages`
    /// pages of filler whose granule positions run up to `samples`.
    pub fn ogg_vorbis(channels: u8, sample_rate: u32, samples: u64, audio_pages: u32) -> Vec<u8> {
        let serial = 0x4E56_4621;
        let mut f = ogg_page(2, 0, serial, 0, &vorbis_id(channels, sample_rate));
        f.extend(ogg_page(0, 0, serial, 1, &[3u8; 40]));
        let n = audio_pages.max(1);
        for k in 1..=n {
            let last = k == n;
            let granule = samples * k as u64 / n as u64;
            f.extend(ogg_page(if last { 4 } else { 0 }, granule, serial, 1 + k, &[0u8; 300]));
        }
        f
    }

    /// A PCM WAV file of silence.
    pub fn wav_pcm(channels: u16, sample_rate: u32, bits: u16, frames: u32) -> Vec<u8> {
        let block = channels * bits / 8;
        let data = frames * block as u32;
        let mut f = b"RIFF".to_vec();
        f.extend((36 + data).to_le_bytes());
        f.extend(b"WAVEfmt ");
        f.extend(16u32.to_le_bytes());
        f.extend(1u16.to_le_bytes());
        f.extend(channels.to_le_bytes());
        f.extend(sample_rate.to_le_bytes());
        f.extend((sample_rate * block as u32).to_le_bytes());
        f.extend(block.to_le_bytes());
        f.extend(bits.to_le_bytes());
        f.extend(b"data");
        f.extend(data.to_le_bytes());
        f.extend(std::iter::repeat_n(0u8, data as usize));
        f
    }
}

#[cfg(test)]
mod tests {
    use super::synth::*;
    use super::*;

    #[test]
    fn ogg_length_is_last_granule_over_rate() {
        let f = ogg_vorbis(1, 44_100, 110_250, 5);
        let i = ogg_vorbis_info(&f).unwrap();
        assert_eq!((i.format, i.channels, i.sample_rate, i.samples), (SoundFormat::OggVorbis, 1, 44_100, 110_250));
        assert!((i.seconds - 2.5).abs() < 1e-12);
        assert_eq!(sound_info(&f).unwrap(), i);
    }

    #[test]
    fn ogg_pages_without_a_packet_end_are_passed_over() {
        let serial = 7;
        let mut f = ogg_page(2, 0, serial, 0, &vorbis_id(2, 22_050));
        f.extend(ogg_page(0, 0, serial, 1, &[3u8; 10]));
        f.extend(ogg_page(0, 22_050, serial, 2, &[0u8; 100]));
        // A page on which no packet ends carries granule -1.
        f.extend(ogg_page(0, u64::MAX, serial, 3, &[0u8; 255]));
        // A page of another stream does not count.
        f.extend(ogg_page(0, 999_999, serial + 1, 0, &[0u8; 20]));
        let i = ogg_vorbis_info(&f).unwrap();
        assert_eq!((i.channels, i.samples), (2, 22_050));
        assert!((i.seconds - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_cut_ogg_keeps_its_last_whole_page() {
        let f = ogg_vorbis(1, 48_000, 96_000, 4);
        let cut = &f[..f.len() - 50];
        let i = ogg_vorbis_info(cut).unwrap();
        assert_eq!(i.samples, 72_000);
    }

    #[test]
    fn ogg_errors() {
        assert_eq!(ogg_vorbis_info(b"RIFF....WAVE"), Err(LengthError::NotOgg));
        assert_eq!(ogg_vorbis_info(&ogg_page(2, 0, 1, 0, b"\x7fFLAC not vorbis at all")), Err(LengthError::NotVorbis));
        assert_eq!(ogg_vorbis_info(&ogg_page(2, 0, 1, 0, &vorbis_id(1, 0))), Err(LengthError::ZeroRate));
        // Headers only: no page says how many samples there are.
        assert!(matches!(ogg_vorbis_info(&ogg_page(2, 0, 1, 0, &vorbis_id(1, 44_100))), Err(LengthError::Missing(_))));
        assert!(matches!(sound_info(b"ID3\x03 mp3"), Err(LengthError::UnknownType(_))));
        assert!(matches!(sound_info(b""), Err(LengthError::UnknownType(_))));
    }

    #[test]
    fn wav_pcm_length() {
        let f = wav_pcm(2, 44_100, 16, 66_150);
        let i = wav_info(&f).unwrap();
        assert_eq!((i.format, i.channels, i.sample_rate, i.samples), (SoundFormat::Wav, 2, 44_100, 66_150));
        assert!((i.seconds - 1.5).abs() < 1e-12);
        assert_eq!(sound_info(&f).unwrap(), i);
    }

    #[test]
    fn wav_chunks_before_the_data_are_skipped() {
        // An odd-sized LIST chunk (padded) between fmt and data.
        let plain = wav_pcm(1, 8_000, 8, 4_000);
        let mut f = plain[..36].to_vec();
        f.extend(b"LIST");
        f.extend(3u32.to_le_bytes());
        f.extend(b"abc\0");
        f.extend(&plain[36..]);
        assert!((wav_info(&f).unwrap().seconds - 0.5).abs() < 1e-12);
    }

    #[test]
    fn wav_compressed_uses_fact_or_byte_rate() {
        // Format 2 (ADPCM): 1,000 bytes of data at 4,000 bytes per second.
        let mut f = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        f.extend(16u32.to_le_bytes());
        f.extend(2u16.to_le_bytes());
        f.extend(1u16.to_le_bytes());
        f.extend(8_000u32.to_le_bytes());
        f.extend(4_000u32.to_le_bytes());
        f.extend(256u16.to_le_bytes());
        f.extend(4u16.to_le_bytes());
        let mut with_fact = f.clone();
        f.extend(b"data");
        f.extend(1_000u32.to_le_bytes());
        f.extend([0u8; 1_000]);
        assert!((wav_info(&f).unwrap().seconds - 0.25).abs() < 1e-12);
        with_fact.extend(b"fact");
        with_fact.extend(4u32.to_le_bytes());
        with_fact.extend(1_600u32.to_le_bytes());
        with_fact.extend(b"data");
        with_fact.extend(1_000u32.to_le_bytes());
        with_fact.extend([0u8; 1_000]);
        assert!((wav_info(&with_fact).unwrap().seconds - 0.2).abs() < 1e-12);
    }

    #[test]
    fn wav_errors_and_short_data() {
        assert_eq!(wav_info(b"OggS"), Err(LengthError::NotWav));
        let f = wav_pcm(1, 8_000, 16, 8_000);
        assert!(matches!(wav_info(&f[..40]), Err(LengthError::Missing(_))));
        // Half of the samples are missing: the length is what is there.
        let half = &f[..44 + 8_000];
        assert!((wav_info(half).unwrap().seconds - 0.5).abs() < 1e-12);
    }

    #[test]
    fn corrupt_headers_do_not_panic() {
        // Every cut of an Ogg and a WAV file: an error or a length, no panic.
        for f in [ogg_vorbis(2, 44_100, 88_200, 3), wav_pcm(2, 22_050, 16, 100)] {
            for n in 0..f.len() {
                let _ = sound_info(&f[..n]);
            }
        }
        // Chunk sizes of 4 GB and 0, and a data chunk larger than the file.
        let mut big = b"RIFF\0\0\0\0WAVEjunk".to_vec();
        big.extend(u32::MAX.to_le_bytes());
        assert!(wav_info(&big).is_err());
        let mut zeros = b"RIFF\0\0\0\0WAVE".to_vec();
        for _ in 0..1000 {
            zeros.extend(b"junk\0\0\0\0");
        }
        assert!(matches!(wav_info(&zeros), Err(LengthError::Missing(_))));
        let mut f = wav_pcm(1, 8_000, 16, 10);
        let data_size = f.len() - 20 - 4;
        f[data_size..data_size + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(wav_info(&f).unwrap().samples, 10);
        // Zero rate and zero block align: refused, or measured some other way, never a
        // division by zero.
        let mut zero_rate = wav_pcm(1, 8_000, 16, 10);
        zero_rate[24..28].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(wav_info(&zero_rate), Err(LengthError::ZeroRate));
        let mut no_align = wav_pcm(1, 8_000, 16, 10);
        no_align[32..34].copy_from_slice(&0u16.to_le_bytes());
        assert!(wav_info(&no_align).unwrap().seconds.is_finite());
        // Garbage after an OggS: refused.
        let mut garbage = b"OggS".to_vec();
        garbage.extend((0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8));
        assert!(sound_info(&garbage).is_err());
    }
}
