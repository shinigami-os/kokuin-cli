use crate::{Failure, SchemaKind};

pub fn run(of: SchemaKind) -> Result<(), Failure> {
    let schema = match of {
        SchemaKind::Record => kokuin_model::json_schema(),
        SchemaKind::Diff => kokuin_model::diff_json_schema(),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
