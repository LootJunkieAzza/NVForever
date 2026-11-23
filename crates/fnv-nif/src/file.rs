//! A whole NIF/KF file: header, raw blocks, footer; schema decoding per block.

use crate::codec::{Codec, CodecError, FileCtx, Path};
use crate::header::{Footer, Header, NifError};
use crate::schema::Schema;
use crate::value::Struct;
use fnv_util::Cursor;
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct RawBlock {
    pub type_name: String,
    /// Byte range of the block within the file.
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct NifFile {
    pub header: Header,
    pub blocks: Vec<RawBlock>,
    pub footer: Footer,
    /// Bytes after the footer (none in the FNV corpus [observed]).
    pub trailing: usize,
    /// Offset where the header ends and the first block starts.
    pub header_len: usize,
    /// The header's block-size table was wrong (its end is not the footer that ends the
    /// file), so the blocks were split by decoding them in order instead (#3198).
    pub size_table_ignored: bool,
}

/// How well the schema explains one block.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockStatus {
    /// Decoded and consumed exactly the stored size.
    Ok,
    /// No definition (or only an abstract one) for this block type.
    Unknown,
    /// Decoded but left bytes over.
    Short { consumed: usize, size: usize },
    /// Decoding failed (overrun, bad count, …).
    Error(CodecError),
}

impl BlockStatus {
    pub fn code(&self) -> &'static str {
        match self {
            BlockStatus::Ok => "ok",
            BlockStatus::Unknown => "unknown",
            BlockStatus::Short { .. } => "short",
            BlockStatus::Error(_) => "error",
        }
    }
}

impl NifFile {
    /// Parse the container. Files without a block-size table (before 20.2.0.5) can only
    /// be split by decoding every block, so they need a schema. A file whose table does not
    /// end at a footer that ends the file, with roots among its blocks, is split the same
    /// way when a schema is given (#3198: a mod's meshes whose table is off by one block's
    /// worth; the blocks decode in order and the real footer follows them) [hypothesis: the
    /// original reads the blocks by their types and does not rely on the table].
    pub fn parse(data: &[u8], schema: Option<&Schema>) -> Result<NifFile, NifError> {
        let mut c = Cursor::new(data);
        let header = Header::parse(&mut c)?;
        let header_len = c.pos;
        match &header.block_sizes {
            Some(sizes) => {
                let by_table = split_by_table(data, &header, header_len, sizes);
                let fits = by_table.as_ref().is_ok_and(|(_, footer, end)| footer_ends_file(footer, *end, data.len(), header.num_blocks));
                if !fits {
                    if let Some(schema) = schema {
                        if let Ok((blocks, footer, end)) = split_by_decoding(data, &header, header_len, schema) {
                            if footer_ends_file(&footer, end, data.len(), header.num_blocks) {
                                return Ok(NifFile { header, blocks, footer, trailing: 0, header_len, size_table_ignored: true });
                            }
                        }
                    }
                }
                let (blocks, footer, end) = by_table?;
                Ok(NifFile { header, blocks, footer, trailing: data.len() - end, header_len, size_table_ignored: false })
            }
            None => {
                let schema = schema.ok_or_else(|| NifError::Invalid("version without block sizes: a schema is needed to split blocks".into()))?;
                let (blocks, footer, end) = split_by_decoding(data, &header, header_len, schema)?;
                Ok(NifFile { header, blocks, footer, trailing: data.len() - end, header_len, size_table_ignored: false })
            }
        }
    }

    pub fn ctx(&self) -> FileCtx<'_> {
        FileCtx {
            version: self.header.version,
            user_version: self.header.user_version,
            bs_version: self.header.bs_version(),
            strings: self.header.strings.as_deref(),
        }
    }

    /// Decode block `i`.
    pub fn decode_block(&self, schema: &Schema, data: &[u8], i: usize) -> Result<Struct, BlockStatus> {
        let b = &self.blocks[i];
        let bytes = &data[b.range.clone()];
        let mut c = Cursor::new(bytes);
        match decode_with(schema, &self.header, &b.type_name, &mut c) {
            Err(DecodeFailure::Unknown) => Err(BlockStatus::Unknown),
            Err(DecodeFailure::Codec(e)) => Err(BlockStatus::Error(e)),
            Ok(s) if c.pos != bytes.len() => {
                let _ = s;
                Err(BlockStatus::Short { consumed: c.pos, size: bytes.len() })
            }
            Ok(s) => Ok(s),
        }
    }

    /// Decode block `i` and re-encode it; `Ok(true)` when the bytes are identical.
    pub fn reencode_block(&self, schema: &Schema, data: &[u8], i: usize) -> Result<bool, BlockStatus> {
        let st = self.decode_block(schema, data, i)?;
        let b = &self.blocks[i];
        let def = schema.block(&b.type_name).ok_or(BlockStatus::Unknown)?;
        let codec = Codec { schema, file: self.ctx(), lineage: &def.lineage };
        let mut out = Vec::with_capacity(b.range.len());
        codec
            .encode_fields(&def.fields, &st, 0, None, &Path::Root(&b.type_name), &mut out)
            .map_err(BlockStatus::Error)?;
        Ok(out == data[b.range.clone()])
    }

    /// Rebuild the file from the parsed header, raw blocks and footer.
    pub fn rewrite(&self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        self.header.write(&mut out);
        for b in &self.blocks {
            out.extend_from_slice(&data[b.range.clone()]);
        }
        self.footer.write(&mut out);
        let end = data.len() - self.trailing;
        out.extend_from_slice(&data[end..]);
        out
    }
}

/// The blocks as the size table cuts them, the footer after them and where it ends.
fn split_by_table(data: &[u8], header: &Header, header_len: usize, sizes: &[u32]) -> Result<(Vec<RawBlock>, Footer, usize), NifError> {
    let mut c = Cursor::new(data);
    c.pos = header_len;
    let mut blocks = Vec::with_capacity(sizes.len());
    for (i, &size) in sizes.iter().enumerate() {
        let start = c.pos;
        c.bytes(size as usize)?;
        blocks.push(RawBlock { type_name: header.block_type_name(i).unwrap_or_default(), range: start..c.pos });
    }
    let footer = Footer::parse(&mut c)?;
    Ok((blocks, footer, c.pos))
}

/// The blocks as decoding them in order cuts them, the footer after them and where it ends.
fn split_by_decoding(data: &[u8], header: &Header, header_len: usize, schema: &Schema) -> Result<(Vec<RawBlock>, Footer, usize), NifError> {
    let n = header.num_blocks as usize;
    let mut blocks = Vec::with_capacity(n);
    let mut c = Cursor::new(data);
    c.pos = header_len;
    for i in 0..n {
        let type_name = header.block_type_name(i).unwrap_or_default();
        let start = c.pos;
        let mut bc = Cursor::new(&data[start..]);
        decode_with(schema, header, &type_name, &mut bc).map_err(|e| match e {
            DecodeFailure::Unknown => NifError::Invalid(format!("block {i} ({type_name}) has no schema definition; cannot split the file")),
            DecodeFailure::Codec(e) => NifError::Invalid(format!("block {i} ({type_name}): {e}")),
        })?;
        c.pos = start + bc.pos;
        blocks.push(RawBlock { type_name, range: start..c.pos });
    }
    let footer = Footer::parse(&mut c)?;
    Ok((blocks, footer, c.pos))
}

/// Whether a footer read up to `end` ends the file and names roots among its blocks.
fn footer_ends_file(footer: &Footer, end: usize, len: usize, num_blocks: u32) -> bool {
    end == len && footer.roots.iter().all(|&r| r >= -1 && (r as i64) < i64::from(num_blocks))
}

enum DecodeFailure {
    Unknown,
    Codec(CodecError),
}

fn decode_with(schema: &Schema, header: &Header, type_name: &str, c: &mut Cursor<'_>) -> Result<Struct, DecodeFailure> {
    let def = schema.block(type_name).filter(|d| !d.is_abstract).ok_or(DecodeFailure::Unknown)?;
    let codec = Codec {
        schema,
        file: FileCtx {
            version: header.version,
            user_version: header.user_version,
            bs_version: header.bs_version(),
            strings: header.strings.as_deref(),
        },
        lineage: &def.lineage,
    };
    codec.decode_fields(&def.fields, c, 0, None, &Path::Root(type_name)).map_err(DecodeFailure::Codec)
}
