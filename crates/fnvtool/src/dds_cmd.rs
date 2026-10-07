use crate::source::process;
use crate::DdsCmd;
use fnv_dds::{DdsHeader, HEADER_LEN};
use serde_json::{json, Value};

pub fn run(cmd: DdsCmd) -> anyhow::Result<Value> {
    match cmd {
        DdsCmd::List { archive } => list(&archive),
    }
}

fn row(path: &str, data: &[u8]) -> Value {
    match DdsHeader::parse(data) {
        Ok(h) => {
            let (format, expected) = match h.pixel_format.format() {
                Ok(f) => (f.name().to_string(), Some(h.expected_size(f))),
                Err(desc) => (desc, None),
            };
            json!({"path": path, "width": h.width, "height": h.height, "mip_count": h.mip_count,
                   "levels": h.levels(), "format": format, "cubemap": h.is_cubemap(), "faces": h.faces(),
                   "expected_size": expected, "actual_size": data.len() - HEADER_LEN, "error": null})
        }
        Err(e) => error_row(path, &e.to_string()),
    }
}

fn error_row(path: &str, e: &str) -> Value {
    json!({"path": path, "width": null, "height": null, "mip_count": null, "levels": null, "format": null,
           "cubemap": null, "faces": null, "expected_size": null, "actual_size": null, "error": e})
}

fn list(archive: &std::path::Path) -> anyhow::Result<Value> {
    let items = process(&[archive.to_path_buf()], &["dds"], None, row)?;
    let files: Vec<Value> = items
        .into_iter()
        .map(|it| it.result.unwrap_or_else(|e| error_row(&it.path, &e)))
        .collect();
    let name = archive.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(json!({"schema": "fnv.dds.listing/1", "archive": name, "files": files}))
}
