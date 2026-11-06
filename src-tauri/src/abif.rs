//! Reader for ABIF (`.ab1`) Sanger chromatogram files.
//!
//! Layout: a 34-byte header holding one directory entry that points at the
//! directory, then 28-byte directory entries (name, number, type, size, data).
//! Data of 4 bytes or less is stored inline in the entry's offset field.

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct AbifTrace {
    pub sample_name: Option<String>,
    /// Base calls, uppercase ASCII.
    pub bases: Vec<u8>,
    /// Phred quality per base.
    pub quals: Vec<u8>,
    /// Trace index of each base call peak.
    pub peaks: Vec<u32>,
    /// Analysed trace channels in A, C, G, T order.
    pub traces: [Vec<i16>; 4],
}

struct Entry {
    name: [u8; 4],
    number: i32,
    elem_type: i16,
    num_elements: i32,
    data_size: i32,
    data_offset: i32,
    entry_offset: usize,
}

fn be_i16(b: &[u8], o: usize) -> Result<i16> {
    Ok(i16::from_be_bytes(b.get(o..o + 2).context("truncated file")?.try_into()?))
}

fn be_i32(b: &[u8], o: usize) -> Result<i32> {
    Ok(i32::from_be_bytes(b.get(o..o + 4).context("truncated file")?.try_into()?))
}

fn read_entry(b: &[u8], o: usize) -> Result<Entry> {
    Ok(Entry {
        name: b.get(o..o + 4).context("truncated directory")?.try_into()?,
        number: be_i32(b, o + 4)?,
        elem_type: be_i16(b, o + 8)?,
        num_elements: be_i32(b, o + 12)?,
        data_size: be_i32(b, o + 16)?,
        data_offset: be_i32(b, o + 20)?,
        entry_offset: o,
    })
}

fn entry_data<'a>(b: &'a [u8], e: &Entry) -> Result<&'a [u8]> {
    let size = usize::try_from(e.data_size).context("negative data size")?;
    if size <= 4 {
        let start = e.entry_offset + 20;
        b.get(start..start + size).context("truncated inline data")
    } else {
        let start = usize::try_from(e.data_offset).context("negative data offset")?;
        b.get(start..start + size).context("data outside file")
    }
}

pub fn parse(bytes: &[u8]) -> Result<AbifTrace> {
    if bytes.len() < 34 || &bytes[0..4] != b"ABIF" {
        bail!("not an ABIF (.ab1) file");
    }
    let root = read_entry(bytes, 6)?;
    let dir_offset = usize::try_from(root.data_offset).context("bad directory offset")?;
    let count = usize::try_from(root.num_elements).context("bad directory size")?;
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        entries.push(read_entry(bytes, dir_offset + i * 28)?);
    }
    let find = |name: &[u8; 4], number: i32| entries.iter().find(|e| &e.name == name && e.number == number);

    let bases_entry = find(b"PBAS", 2).or_else(|| find(b"PBAS", 1)).context("file has no base calls (PBAS)")?;
    let bases: Vec<u8> = entry_data(bytes, bases_entry)?.iter().map(|c| c.to_ascii_uppercase()).collect();

    let quals = match find(b"PCON", 2).or_else(|| find(b"PCON", 1)) {
        Some(e) => entry_data(bytes, e)?.to_vec(),
        None => bail!("file has no quality values (PCON)"),
    };
    if quals.len() != bases.len() {
        bail!("base calls ({}) and quality values ({}) differ in length", bases.len(), quals.len());
    }

    let peaks = match find(b"PLOC", 2).or_else(|| find(b"PLOC", 1)) {
        Some(e) => entry_data(bytes, e)?
            .chunks_exact(2)
            .map(|c| i16::from_be_bytes([c[0], c[1]]) as u16 as u32)
            .collect(),
        None => Vec::new(),
    };

    let order: Vec<u8> = match find(b"FWO_", 1) {
        Some(e) => entry_data(bytes, e)?.to_vec(),
        None => b"GATC".to_vec(),
    };
    let mut traces: [Vec<i16>; 4] = Default::default();
    for (i, base) in order.iter().take(4).enumerate() {
        let Some(e) = find(b"DATA", 9 + i as i32) else { continue };
        let channel: Vec<i16> =
            entry_data(bytes, e)?.chunks_exact(2).map(|c| i16::from_be_bytes([c[0], c[1]])).collect();
        let slot = match base.to_ascii_uppercase() {
            b'A' => 0,
            b'C' => 1,
            b'G' => 2,
            b'T' => 3,
            _ => continue,
        };
        traces[slot] = channel;
    }

    let sample_name = find(b"SMPL", 1).and_then(|e| {
        let data = entry_data(bytes, e).ok()?;
        // pString (type 18): length byte, then text. cString (19): NUL-terminated.
        let text = if e.elem_type == 18 && !data.is_empty() {
            let n = (data[0] as usize).min(data.len() - 1);
            &data[1..1 + n]
        } else {
            data.split(|&c| c == 0).next().unwrap_or(&[])
        };
        let s = String::from_utf8_lossy(text).trim().to_string();
        (!s.is_empty()).then_some(s)
    });

    Ok(AbifTrace { sample_name, bases, quals, peaks, traces })
}
