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
    /// be split by decoding every block, so they need a schema.
    pub fn parse(data: &[u8], schema: Option<&Schema>) -> Result<NifFile, NifError> {
        let mut c = Cursor::new(data);
        let mut header = Header::parse(&mut c)?;
        let header_len = c.pos;
        match Self::split(data, header_len, &header, schema, true) {
            Ok((blocks, footer, trailing)) if trailing == 0 || header.block_sizes.is_none() || schema.is_none() => {
                Ok(NifFile { header, blocks, footer, trailing, header_len })
            }
            first => {
                // The block-size table is a convenience for skipping blocks; the blocks
                // themselves say how long they are. Some mod tools write a wrong table
                // [observed: Heart of the Mojave ultraluxhotelfront.nif and
                // ultraluxdome_glowsbackside.nif store NiTriStripsData sizes without the
                // strip points; NV Interiors hulagirl1.nif's sizes overrun the file by 35
                // bytes], so when the stored sizes do not end in a footer at the end of the
                // file, split again by decoding each known block with the schema.
                let Ok((blocks, footer, trailing)) = Self::split(data, header_len, &header, schema, false) else {
                    return first.map(|(blocks, footer, trailing)| NifFile { header, blocks, footer, trailing, header_len });
                };
                if trailing != 0 {
                    if let Ok((blocks, footer, trailing)) = first {
                        return Ok(NifFile { header, blocks, footer, trailing, header_len });
                    }
                }
                // Keep the header consistent with the blocks so `rewrite` makes a valid file.
                header.block_sizes = Some(blocks.iter().map(|b| b.range.len() as u32).collect());
                Ok(NifFile { header, blocks, footer, trailing, header_len })
            }
        }
    }

    /// Split the blocks after the header and read the footer: by the stored block sizes
    /// when `trust_sizes` and the file has them, otherwise by decoding each block with the
    /// schema (a block the schema does not know falls back to its stored size, if any).
    fn split(
        data: &[u8],
        header_len: usize,
        header: &Header,
        schema: Option<&Schema>,
        trust_sizes: bool,
    ) -> Result<(Vec<RawBlock>, Footer, usize), NifError> {
        let mut c = Cursor::new(data);
        c.pos = header_len;
        let n = header.num_blocks as usize;
        let mut blocks = Vec::with_capacity(n);
        if !trust_sizes {
            let schema = schema.ok_or_else(|| NifError::Invalid("a schema is needed to split blocks".into()))?;
            let sizes = header.block_sizes.as_deref();
            for i in 0..n {
                let type_name = header.block_type_name(i).unwrap_or_default();
                let start = c.pos;
                let mut bc = Cursor::new(&data[start..]);
                match decode_with(schema, header, &type_name, &mut bc) {
                    Ok(_) => c.pos = start + bc.pos,
                    Err(DecodeFailure::Unknown) => match sizes.and_then(|s| s.get(i)) {
                        Some(&size) => {
                            c.bytes(size as usize)?;
                        }
                        None => {
                            return Err(NifError::Invalid(format!(
                                "block {i} ({type_name}) has no schema definition; cannot split the file"
                            )))
                        }
                    },
                    Err(DecodeFailure::Codec(e)) => return Err(NifError::Invalid(format!("block {i} ({type_name}): {e}"))),
                }
                blocks.push(RawBlock { type_name, range: start..c.pos });
            }
            let footer = Footer::parse(&mut c)?;
            return Ok((blocks, footer, c.remaining()));
        }
        match &header.block_sizes {
            Some(sizes) => {
                for (i, &size) in sizes.iter().enumerate() {
                    let start = c.pos;
                    c.bytes(size as usize)?;
                    blocks.push(RawBlock { type_name: header.block_type_name(i).unwrap_or_default(), range: start..c.pos });
                }
            }
            None => {
                let schema = schema.ok_or_else(|| {
                    NifError::Invalid("version without block sizes: a schema is needed to split blocks".into())
                })?;
                for i in 0..n {
                    let type_name = header.block_type_name(i).unwrap_or_default();
                    let start = c.pos;
                    let mut bc = Cursor::new(&data[start..]);
                    decode_with(schema, header, &type_name, &mut bc).map_err(|e| match e {
                        DecodeFailure::Unknown => {
                            NifError::Invalid(format!("block {i} ({type_name}) has no schema definition; cannot split the file"))
                        }
                        DecodeFailure::Codec(e) => NifError::Invalid(format!("block {i} ({type_name}): {e}")),
                    })?;
                    c.pos = start + bc.pos;
                    blocks.push(RawBlock { type_name, range: start..c.pos });
                }
            }
        }
        let footer = Footer::parse(&mut c)?;
        Ok((blocks, footer, c.remaining()))
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
