use crate::input::Input;
use crate::spec::Spec;

/// The context block shared by every LLM call.
/// Order: campaign context → brand guide (conventions) → requirements → content type → per-block content.
pub fn shared_context(spec: &Spec, input: &Input) -> String {
    let mut c = String::new();
    c.push_str(&format!("## Campaign context\n{}\n\n", spec.context));
    if let Some(conv) = &input.conventions {
        c.push_str(&format!("## Brand guide (verbatim, takes precedence after explicit requirements)\n{}\n\n", conv));
    }
    if let Some(req) = &input.requirements {
        c.push_str(&format!("## Requirements\n{}\n\n", req));
    }
    c.push_str(&format!("## Content type\n{}\n\n", input.content_type));
    c.push_str("## Content by block\n");
    for (block_id, block_content) in &input.blocks {
        c.push_str(&format!("### {block_id}\n{block_content}\n\n"));
    }
    c
}
