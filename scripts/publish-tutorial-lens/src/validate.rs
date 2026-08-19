//! Exercises runtime dispatch and validation for a bundled dialect record.

use anyhow::{Result, bail};
use idiolect_records::{AnyRecord, Dialect, Record, decode_record, examples};

fn main() -> Result<()> {
    let mut value = serde_json::to_value(examples::dialect())?;
    let invalid = std::env::args().any(|arg| arg == "--invalid");

    if invalid {
        value
            .as_object_mut()
            .expect("a generated record serializes as an object")
            .remove("createdAt");
    }

    match decode_record(&Dialect::nsid(), value) {
        Ok(AnyRecord::Dialect(dialect)) if !invalid => {
            println!("validated {}: {}", Dialect::NSID, dialect.name);
        }
        Err(error) if invalid => {
            println!("rejected invalid {} record: {error}", Dialect::NSID);
        }
        Ok(_) if invalid => bail!("invalid record unexpectedly passed validation"),
        Ok(other) => bail!("expected a dialect, got {}", other.nsid_str()),
        Err(error) => return Err(error.into()),
    }

    Ok(())
}
