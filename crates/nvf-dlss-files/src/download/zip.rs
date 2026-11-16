//! The parts of a zip archive the download reads, from bytes already fetched: the end record,
//! the central directory, the entries for the listed files and the byte ranges that hold them.
//! Pure: no I/O. The layout is PKWARE's APPNOTE.TXT 6.3.10 [documented: §4.3.6 the overall
//! layout, §4.3.7 the local file header, §4.3.12 the central directory header, §4.3.14 the
//! ZIP64 end of central directory record, §4.3.15 its locator, §4.3.16 the end of central
//! directory record, §4.4.4 the general purpose flags (bit 0 encrypted, bit 3 sizes in a data
//! descriptor), §4.4.5 the methods (0 stored, 8 deflated), §4.3.9 the data descriptor, §4.5.3
//! the ZIP64 extended information extra field 0x0001]. All numbers little-endian.
//!
//! [design] The sizes, CRC and offset of an entry are always the central directory's: the
//! local header's may be zero (bit 3). Each listed file's range runs from its local header to
//! the next local header in offset order (or the directory's start), so one request covers the
//! header, whose name and extra lengths are known only once it is read, the data and any data
//! descriptor; ranges that touch are merged into one request. The names written are never the
//! archive's: the caller names files from its own list.

use fnv_util::Cursor;

use super::{DownloadFile, Failure};

const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const EOCD_LEN: usize = 22;
const ZIP64_LOCATOR_LEN: usize = 20;
/// The ZIP64 end record's fixed part, the bytes [`parse_zip64_end`] reads.
pub(super) const ZIP64_EOCD_LEN: usize = 56;
/// The local file header's fixed part (the name and extra field follow).
pub(super) const LOCAL_HEADER_LEN: usize = 30;
const CENTRAL_LEN: usize = 46;
/// A data descriptor at its longest: signature, CRC and two 8-byte sizes (§4.3.9).
const DESCRIPTOR_MAX: u64 = 24;
/// The archive's last bytes read to find its end: the end record with the longest comment and
/// a ZIP64 locator before it.
pub(super) const TAIL_LEN: u64 = (EOCD_LEN + 0xFFFF + ZIP64_LOCATOR_LEN) as u64;
/// The largest central directory read [design: the official archive's 511 entries need a few
/// tens of KiB].
pub(super) const MAX_DIRECTORY: u64 = 16 << 20;
/// The largest listed entry, packed or unpacked [design: `nvngx_dlss.dll` is 56 MiB].
pub(super) const MAX_ENTRY: u64 = 128 << 20;
const FLAG_ENCRYPTED: u16 = 1;
/// The method of an entry kept as it is.
pub(super) const STORED: u16 = 0;
/// The method of a deflated entry.
pub(super) const DEFLATED: u16 = 8;
const ZIP64_EXTRA: u16 = 0x0001;

/// Where the central directory is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Directory {
    pub(super) offset: u64,
    pub(super) size: u64,
    pub(super) entries: u64,
}

/// What the archive's end says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum End {
    /// The directory, from the end record.
    Directory(Directory),
    /// A ZIP64 archive: the ZIP64 end record is at this offset ([`ZIP64_EOCD_LEN`] bytes, read
    /// by [`parse_zip64_end`]).
    Zip64At(u64),
}

fn short(e: fnv_util::OutOfBounds) -> String {
    format!("a record ends early ({e})")
}

/// The end record found in `tail`, the archive's last bytes, which start at `tail_start`: the
/// last record signature whose comment ends exactly at the archive's end.
pub(super) fn find_end(tail: &[u8], tail_start: u64) -> Result<End, String> {
    let at = (0..=tail.len().saturating_sub(EOCD_LEN))
        .rev()
        .filter(|&pos| tail.len() >= pos + EOCD_LEN)
        .find(|&pos| {
            let word = |o: usize| u32::from_le_bytes([tail[o], tail[o + 1], tail[o + 2], tail[o + 3]]);
            let comment = u16::from_le_bytes([tail[pos + 20], tail[pos + 21]]) as usize;
            word(pos) == EOCD_SIG && pos + EOCD_LEN + comment == tail.len()
        })
        .ok_or("no end of central directory record in its last bytes")?;
    let mut c = Cursor::new(&tail[at + 4..]);
    let disk = c.u16().map_err(short)?;
    let directory_disk = c.u16().map_err(short)?;
    let _entries_here = c.u16().map_err(short)?;
    let entries = c.u16().map_err(short)?;
    let size = c.u32().map_err(short)?;
    let offset = c.u32().map_err(short)?;
    if entries == 0xFFFF || size == u32::MAX || offset == u32::MAX {
        let locator = at.checked_sub(ZIP64_LOCATOR_LEN).ok_or("a ZIP64 archive without its locator")?;
        let mut c = Cursor::new(&tail[locator..at]);
        if c.u32().map_err(short)? != ZIP64_LOCATOR_SIG {
            return Err("a ZIP64 archive without its locator".into());
        }
        let _disk = c.u32().map_err(short)?;
        let record = c.u64().map_err(short)?;
        let disks = c.u32().map_err(short)?;
        if disks > 1 {
            return Err("the archive spans several disks".into());
        }
        let locator_at = tail_start + locator as u64;
        if record.checked_add(ZIP64_EOCD_LEN as u64).is_none_or(|end| end > locator_at) {
            return Err("the ZIP64 end record is not before its locator".into());
        }
        return Ok(End::Zip64At(record));
    }
    if disk != 0 || directory_disk != 0 {
        return Err("the archive spans several disks".into());
    }
    checked(Directory { offset: offset.into(), size: size.into(), entries: entries.into() }, tail_start + at as u64).map(End::Directory)
}

/// The directory from the ZIP64 end record (`bytes`, read at `at`).
pub(super) fn parse_zip64_end(bytes: &[u8], at: u64) -> Result<Directory, String> {
    let mut c = Cursor::new(bytes);
    if c.u32().map_err(short)? != ZIP64_EOCD_SIG {
        return Err("no ZIP64 end record where its locator points".into());
    }
    let _record_size = c.u64().map_err(short)?;
    let _made_by = c.u16().map_err(short)?;
    let _needed = c.u16().map_err(short)?;
    let disk = c.u32().map_err(short)?;
    let directory_disk = c.u32().map_err(short)?;
    let _entries_here = c.u64().map_err(short)?;
    let entries = c.u64().map_err(short)?;
    let size = c.u64().map_err(short)?;
    let offset = c.u64().map_err(short)?;
    if disk != 0 || directory_disk != 0 {
        return Err("the archive spans several disks".into());
    }
    checked(Directory { offset, size, entries }, at)
}

/// `dir` if it ends at or before `end_at` (where the end records start) and is not too large.
fn checked(dir: Directory, end_at: u64) -> Result<Directory, String> {
    if dir.size > MAX_DIRECTORY {
        return Err(format!("its central directory is {} bytes, more than {MAX_DIRECTORY}", dir.size));
    }
    if dir.offset.checked_add(dir.size).is_none_or(|end| end > end_at) {
        return Err("its central directory runs past the end records".into());
    }
    Ok(dir)
}

/// One central directory entry, the parts the download uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    /// As the archive spells it, backslashes made slashes.
    pub(super) name: String,
    pub(super) method: u16,
    pub(super) flags: u16,
    pub(super) crc32: u32,
    pub(super) compressed: u64,
    pub(super) uncompressed: u64,
    pub(super) local_offset: u64,
}

/// The entries of the central directory `bytes` (all of `dir.size`, read at `dir.offset`).
pub(super) fn read_directory(bytes: &[u8], dir: &Directory) -> Result<Vec<Entry>, String> {
    let mut c = Cursor::new(bytes);
    let mut entries = Vec::new();
    for _ in 0..dir.entries {
        if c.remaining() < CENTRAL_LEN || c.u32().map_err(short)? != CENTRAL_SIG {
            return Err(format!("its central directory has {} of its {} entries", entries.len(), dir.entries));
        }
        let _made_by = c.u16().map_err(short)?;
        let _needed = c.u16().map_err(short)?;
        let flags = c.u16().map_err(short)?;
        let method = c.u16().map_err(short)?;
        let _time = c.u16().map_err(short)?;
        let _date = c.u16().map_err(short)?;
        let crc32 = c.u32().map_err(short)?;
        let compressed32 = c.u32().map_err(short)?;
        let uncompressed32 = c.u32().map_err(short)?;
        let name_len = c.u16().map_err(short)? as usize;
        let extra_len = c.u16().map_err(short)? as usize;
        let comment_len = c.u16().map_err(short)? as usize;
        let disk32 = c.u16().map_err(short)?;
        let _internal = c.u16().map_err(short)?;
        let _external = c.u32().map_err(short)?;
        let offset32 = c.u32().map_err(short)?;
        let name = String::from_utf8_lossy(c.bytes(name_len).map_err(short)?).replace('\\', "/");
        let extra = c.bytes(extra_len).map_err(short)?;
        c.bytes(comment_len).map_err(short)?;
        // The ZIP64 field holds, in this order, each value saturated in the fixed part (§4.5.3).
        let mut uncompressed = u64::from(uncompressed32);
        let mut compressed = u64::from(compressed32);
        let mut local_offset = u64::from(offset32);
        let mut disk = u32::from(disk32);
        if uncompressed32 == u32::MAX || compressed32 == u32::MAX || offset32 == u32::MAX || disk32 == 0xFFFF {
            let mut field = zip64_field(extra).ok_or_else(|| format!("{name}: saturated sizes without a ZIP64 field"))?;
            let missing = || format!("{name}: its ZIP64 field is too short");
            if uncompressed32 == u32::MAX {
                uncompressed = field.u64().map_err(|_| missing())?;
            }
            if compressed32 == u32::MAX {
                compressed = field.u64().map_err(|_| missing())?;
            }
            if offset32 == u32::MAX {
                local_offset = field.u64().map_err(|_| missing())?;
            }
            if disk32 == 0xFFFF {
                disk = field.u32().map_err(|_| missing())?;
            }
        }
        if disk != 0 {
            return Err(format!("{name} is on another disk"));
        }
        if local_offset >= dir.offset {
            return Err(format!("{name}: its local header is not before the central directory"));
        }
        entries.push(Entry { name, method, flags, crc32, compressed, uncompressed, local_offset });
    }
    Ok(entries)
}

/// The ZIP64 extra field's data in an extra block, if it has one.
fn zip64_field(extra: &[u8]) -> Option<Cursor<'_>> {
    let mut c = Cursor::new(extra);
    while c.remaining() >= 4 {
        let id = c.u16().ok()?;
        let len = c.u16().ok()? as usize;
        let data = c.bytes(len).ok()?;
        if id == ZIP64_EXTRA {
            return Some(Cursor::new(data));
        }
    }
    None
}

/// A listed file and the one entry that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Selected {
    pub(super) file: DownloadFile,
    pub(super) entry: Entry,
}

/// For each listed file, in the list's order, its entry: exactly one entry named its archive
/// path, stored or deflated, not encrypted, of the pinned size and within [`MAX_ENTRY`].
pub(super) fn select(entries: &[Entry], files: &[DownloadFile]) -> Result<Vec<Selected>, Failure> {
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let wanted = file.zip_path.replace('\\', "/");
        let mut found = entries.iter().filter(|e| e.name == wanted);
        let entry = found.next().ok_or(Failure::EntryMissing(file.zip_path))?;
        if found.next().is_some() {
            return Err(Failure::NotAZip(format!("it has more than one entry {}", file.zip_path)));
        }
        let bad = |why: String| Err(Failure::NotAZip(format!("{}: {why}", file.zip_path)));
        if entry.flags & FLAG_ENCRYPTED != 0 {
            return bad("encrypted".into());
        }
        if entry.method != STORED && entry.method != DEFLATED {
            return bad(format!("packed with method {}, not stored or deflated", entry.method));
        }
        if entry.uncompressed != file.size {
            return bad(format!("{} bytes, not the pinned {}", entry.uncompressed, file.size));
        }
        if entry.uncompressed > MAX_ENTRY || entry.compressed > MAX_ENTRY {
            return bad(format!("larger than {MAX_ENTRY} bytes"));
        }
        if entry.method == STORED && entry.compressed != entry.uncompressed {
            return bad("stored with two different sizes".into());
        }
        out.push(Selected { file: *file, entry: entry.clone() });
    }
    Ok(out)
}

/// One byte range to fetch: `[start, end)` of the archive, holding the listed entries
/// `items` in offset order: each one's index into the selection and the end of its own part
/// of the range (the next local header's offset, or the directory's), which its header, name,
/// extra field and data must not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) items: Vec<(usize, u64)>,
}

impl Span {
    pub(super) fn len(&self) -> u64 {
        self.end - self.start
    }
}

/// The ranges to fetch for `selected`: each entry from its local header to the next local
/// header of any entry (or the directory's start), touching ranges merged. A range too short
/// for the header, the name and the data, or longer than they and a data descriptor can be,
/// is refused.
pub(super) fn plan(selected: &[Selected], entries: &[Entry], dir: &Directory) -> Result<Vec<Span>, String> {
    let mut offsets: Vec<u64> = entries.iter().map(|e| e.local_offset).collect();
    offsets.sort_unstable();
    let mut order: Vec<usize> = (0..selected.len()).collect();
    order.sort_by_key(|&i| selected[i].entry.local_offset);
    let mut spans: Vec<Span> = Vec::new();
    for i in order {
        let entry = &selected[i].entry;
        let start = entry.local_offset;
        let next = offsets.partition_point(|&o| o <= start);
        if offsets[..next].iter().filter(|&&o| o == start).count() > 1 {
            return Err(format!("{}: two entries share its local header", entry.name));
        }
        let end = offsets.get(next).copied().unwrap_or(dir.offset);
        let least = LOCAL_HEADER_LEN as u64 + entry.name.len() as u64 + entry.compressed;
        let most = LOCAL_HEADER_LEN as u64 + 2 * 0xFFFF + entry.compressed + DESCRIPTOR_MAX;
        let len = end - start;
        if len < least {
            return Err(format!("{}: {len} bytes to the next header, too few for its data", entry.name));
        }
        if len > most {
            return Err(format!("{}: {len} bytes to the next header, more than its header and data can be", entry.name));
        }
        match spans.last_mut() {
            Some(last) if last.end == start => {
                last.end = end;
                last.items.push((i, end));
            }
            _ => spans.push(Span { start, end, items: vec![(i, end)] }),
        }
    }
    Ok(spans)
}

/// A local file header's fixed part, the parts the download checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Local {
    pub(super) method: u16,
    pub(super) flags: u16,
    pub(super) name_len: usize,
    pub(super) extra_len: usize,
}

/// The local header whose fixed part is `bytes`.
pub(super) fn local_header(bytes: &[u8; LOCAL_HEADER_LEN]) -> Result<Local, String> {
    let mut c = Cursor::new(bytes);
    if c.u32().map_err(short)? != LOCAL_SIG {
        return Err("no local file header where the directory points".into());
    }
    let _needed = c.u16().map_err(short)?;
    let flags = c.u16().map_err(short)?;
    let method = c.u16().map_err(short)?;
    c.bytes(16).map_err(short)?; // time, date, CRC, sizes: the directory's are used
    let name_len = c.u16().map_err(short)? as usize;
    let extra_len = c.u16().map_err(short)? as usize;
    Ok(Local { method, flags, name_len, extra_len })
}

#[cfg(test)]
mod tests {
    use super::super::test_zip::{self, Options, TestEntry};
    use super::*;

    const FILES: [DownloadFile; 2] = [
        DownloadFile { name: "a.dll", zip_path: "bin/x64/a.dll", size: 3000, sha256: "" },
        DownloadFile { name: "c.dll", zip_path: "bin/x64/c.dll", size: 5, sha256: "" },
    ];

    fn sample(options: &Options) -> Vec<u8> {
        let a: Vec<u8> = (0..3000u32).map(|i| (i % 7) as u8).collect();
        test_zip::build(
            &[
                TestEntry::deflated("bin/x64/a.dll", &a),
                TestEntry::stored("bin/x64/b.dll", b"not listed"),
                TestEntry::stored("bin\\x64\\c.dll", b"hello"),
                TestEntry::deflated("bin/x64/nvngx_update.exe", b"never"),
            ],
            options,
        )
    }

    /// The directory and its entries, read as the download reads them (tail, then directory).
    fn read(zip: &[u8]) -> (Directory, Vec<Entry>) {
        let tail_start = (zip.len() as u64).saturating_sub(TAIL_LEN);
        let dir = match find_end(&zip[tail_start as usize..], tail_start).unwrap() {
            End::Directory(dir) => dir,
            End::Zip64At(at) => parse_zip64_end(&zip[at as usize..at as usize + ZIP64_EOCD_LEN], at).unwrap(),
        };
        let entries = read_directory(&zip[dir.offset as usize..(dir.offset + dir.size) as usize], &dir).unwrap();
        (dir, entries)
    }

    #[test]
    fn the_end_record_is_found_behind_a_long_comment() {
        for comment in [0usize, 1, 300, 0xFFFF] {
            let zip = sample(&Options { comment: vec![b'P'; comment], ..Options::default() });
            let (dir, entries) = read(&zip);
            assert_eq!(dir.entries, 4, "comment {comment}");
            assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["bin/x64/a.dll", "bin/x64/b.dll", "bin/x64/c.dll", "bin/x64/nvngx_update.exe"]);
            assert_eq!(entries[2].method, STORED);
            assert_eq!(entries[0].method, DEFLATED);
            assert_eq!(entries[0].uncompressed, 3000);
        }
    }

    #[test]
    fn a_comment_holding_a_fake_end_record_is_not_taken_for_the_record() {
        // A record signature inside the comment whose own comment length does not reach the end.
        let mut comment = vec![0x50, 0x4b, 0x05, 0x06];
        comment.extend([0u8; 30]);
        let zip = sample(&Options { comment, ..Options::default() });
        assert_eq!(read(&zip).0.entries, 4);
    }

    #[test]
    fn zip64_records_and_extra_fields_are_read() {
        let plain = sample(&Options::default());
        let zip = sample(&Options { zip64: true, ..Options::default() });
        let tail_start = (zip.len() as u64).saturating_sub(TAIL_LEN);
        assert!(matches!(find_end(&zip[tail_start as usize..], tail_start), Ok(End::Zip64At(_))));
        let (dir, entries) = read(&zip);
        let (_, plain_entries) = read(&plain);
        assert_eq!(dir.entries, 4);
        for (z, p) in entries.iter().zip(&plain_entries) {
            assert_eq!((z.name.as_str(), z.crc32, z.compressed, z.uncompressed, z.local_offset), (p.name.as_str(), p.crc32, p.compressed, p.uncompressed, p.local_offset));
        }
    }

    #[test]
    fn broken_ends_are_refused() {
        let zip = sample(&Options::default());
        assert!(find_end(&zip[..zip.len() - 1], 0).is_err(), "the record cut short");
        assert!(find_end(b"no zip here at all, just words", 0).is_err());
        // The directory said to run past the end record.
        let mut bad = zip.clone();
        let at = bad.len() - EOCD_LEN;
        let len = bad.len() as u32;
        bad[at + 12..at + 16].copy_from_slice(&len.to_le_bytes());
        assert!(find_end(&bad, 0).unwrap_err().contains("past the end"));
        // A ZIP64 marker without its locator.
        let mut bad = zip;
        let at = bad.len() - EOCD_LEN;
        bad[at + 16..at + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(find_end(&bad, 0).unwrap_err().contains("locator"));
    }

    #[test]
    fn selection_takes_exactly_the_listed_entries() {
        let zip = sample(&Options::default());
        let (_, entries) = read(&zip);
        let selected = select(&entries, &FILES).unwrap();
        assert_eq!(selected.iter().map(|s| s.file.name).collect::<Vec<_>>(), ["a.dll", "c.dll"]);
        assert_eq!(selected[1].entry.name, "bin/x64/c.dll", "backslashes read as slashes");
        let missing = [DownloadFile { name: "d.dll", zip_path: "bin/x64/d.dll", size: 1, sha256: "" }];
        assert_eq!(select(&entries, &missing), Err(Failure::EntryMissing("bin/x64/d.dll")));
        let wrong_size = [DownloadFile { size: 6, ..FILES[1] }];
        assert!(matches!(select(&entries, &wrong_size), Err(Failure::NotAZip(why)) if why.contains("not the pinned 6")));
        let mut twice = entries.clone();
        twice.push(entries[2].clone());
        assert!(matches!(select(&twice, &FILES), Err(Failure::NotAZip(why)) if why.contains("more than one")));
        let mut odd = entries.clone();
        odd[0].method = 12;
        assert!(matches!(select(&odd, &FILES), Err(Failure::NotAZip(why)) if why.contains("method 12")));
        let mut locked = entries;
        locked[2].flags |= FLAG_ENCRYPTED;
        assert!(matches!(select(&locked, &FILES), Err(Failure::NotAZip(why)) if why.contains("encrypted")));
    }

    #[test]
    fn the_plan_covers_only_the_listed_entries_and_merges_touching_ones() {
        for descriptor in [false, true] {
            let zip = sample(&Options { descriptor, ..Options::default() });
            let (dir, entries) = read(&zip);
            // a.dll and c.dll are apart (b.dll between): two ranges, each to the next header.
            let selected = select(&entries, &FILES).unwrap();
            let spans = plan(&selected, &entries, &dir).unwrap();
            assert_eq!(spans, [Span { start: entries[0].local_offset, end: entries[1].local_offset, items: vec![(0, entries[1].local_offset)] }, Span {
                start: entries[2].local_offset,
                end: entries[3].local_offset,
                items: vec![(1, entries[3].local_offset)]
            }]);
            // b.dll and c.dll touch: one range, from b's header to the updater's.
            let touching = [DownloadFile { name: "c.dll", zip_path: "bin/x64/c.dll", size: 5, sha256: "" }, DownloadFile {
                name: "b.dll",
                zip_path: "bin/x64/b.dll",
                size: 10,
                sha256: "",
            }];
            let selected = select(&entries, &touching).unwrap();
            let spans = plan(&selected, &entries, &dir).unwrap();
            assert_eq!(spans, [Span {
                start: entries[1].local_offset,
                end: entries[3].local_offset,
                items: vec![(1, entries[2].local_offset), (0, entries[3].local_offset)]
            }]);
            // The last entry's range ends at the directory.
            let last = [DownloadFile { name: "u", zip_path: "bin/x64/nvngx_update.exe", size: 5, sha256: "" }];
            let selected = select(&entries, &last).unwrap();
            assert_eq!(plan(&selected, &entries, &dir).unwrap()[0].end, dir.offset);
            // Each range starts with a local header naming the entry.
            let header: &[u8; LOCAL_HEADER_LEN] = (&zip[entries[2].local_offset as usize..][..LOCAL_HEADER_LEN]).try_into().unwrap();
            let local = local_header(header).unwrap();
            assert_eq!((local.method, local.name_len), (STORED, "bin\\x64\\c.dll".len()));
            assert_eq!(local.flags & (1 << 3) != 0, descriptor);
        }
    }

    #[test]
    fn a_range_that_cannot_hold_its_entry_is_refused() {
        let zip = sample(&Options::default());
        let (dir, entries) = read(&zip);
        let selected = select(&entries, &FILES).unwrap();
        let mut squeezed = entries.clone();
        squeezed[1].local_offset = entries[0].local_offset + 40;
        assert!(plan(&selected, &squeezed, &dir).unwrap_err().contains("too few"));
        // The last entry's range runs to the directory: put the directory far after it.
        let last = [DownloadFile { name: "u", zip_path: "bin/x64/nvngx_update.exe", size: 5, sha256: "" }];
        let far = Directory { offset: dir.offset + 200_000, ..dir };
        assert!(plan(&select(&entries, &last).unwrap(), &entries, &far).unwrap_err().contains("more than"));
        let mut shared = entries;
        shared[1].local_offset = shared[0].local_offset;
        assert!(plan(&selected, &shared, &dir).unwrap_err().contains("share"));
        assert!(local_header(&[0u8; LOCAL_HEADER_LEN]).is_err());
    }
}
