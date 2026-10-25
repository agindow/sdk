//! TI-38: snapshot precision adapters. Compile after local snapshot selection,
//! outside forwarding. No registry loading, I/O or default-engine wiring.
use agw_audit::contracts::contract_snapshot::Tool;
use agw_inspector::{FieldPrecisions, PrecisionError, inference::Schema};

/// Compile hints from one already-validated, selected tool. Duplicate or
/// oversized field sets fail explicitly at the administrative load boundary.
/// An absent or false flag is Observed; a container flag never propagates.
pub fn precision_for(tool: &Tool) -> Result<FieldPrecisions, PrecisionError> {
    FieldPrecisions::new(
        tool.fields
            .iter()
            .map(|field| (field.path.as_str(), field.high_precision)),
    )
}

/// Annotate an owned draft from published live-sample evidence. Structural
/// metadata stays caller-supplied; unions are not collapsed into a wire type.
/// Explicit flags (including false) take precedence over learned hints.
pub fn with_learned_precision(tool: &Tool, schema: &Schema) -> Tool {
    let paths = schema.precision_paths();
    let mut tool = tool.clone();
    for field in &mut tool.fields {
        if field.high_precision.is_none() {
            field.high_precision = paths
                .get(field.path.as_str())
                .and_then(|node| node.high_precision);
        }
    }
    tool
}
