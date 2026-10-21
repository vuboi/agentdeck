//! Regenerates `packages/protocol-ts/src/index.ts` from the Rust protocol types.
//! Run: `cargo run -p ag-protocol --example export_ts`

use std::{fs, path::Path};

fn main() {
    let ts = ag_protocol::export_typescript().expect("TypeScript export failed");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/protocol-ts/src/index.ts");
    fs::write(&out, ts).expect("write index.ts");
    println!("wrote {}", out.display());
}
