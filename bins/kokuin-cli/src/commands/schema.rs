use crate::Failure;

pub fn run() -> Result<(), Failure> {
    println!(
        "{}",
        serde_json::to_string_pretty(&kokuin_model::json_schema())?
    );
    Ok(())
}
