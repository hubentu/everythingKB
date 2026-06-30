pub fn system_schema(language: &str, schema_md: &str) -> String {
    format!(
        "You are everythingKB's wiki compilation agent for a personal knowledge base.\n\n\
         {schema_md}\n\n\
         Write all content in {language}.\n\
         Use OKF markdown links to connect related pages (e.g. [attention](concepts/attention.md)).\n\
         Return ONLY valid JSON when asked for JSON."
    )
}

/// Max source chars sent to the compiler LLM (leaves context for JSON output).
const MAX_COMPILE_CHARS: usize = 12_000;

fn clip_content(content: &str) -> String {
    let mut char_count = 0usize;
    let mut clip_at = content.len();
    for (byte_idx, _) in content.char_indices() {
        if char_count == MAX_COMPILE_CHARS {
            clip_at = byte_idx;
            break;
        }
        char_count += 1;
    }
    if clip_at == content.len() {
        return content.to_string();
    }
    let omitted = content[clip_at..].chars().count();
    format!(
        "{}\n\n[... {omitted} chars omitted — summarize from this excerpt ...]",
        &content[..clip_at]
    )
}

pub fn image_summary_user(doc_name: &str, path: &std::path::Path) -> String {
    format!(
        "Image file: {doc_name}\nPath: {}\n\n\
         Describe what you see in this image for a personal knowledge base.\n\
         Include: main subjects, text visible in the image, setting, notable details, \
         and anything worth remembering later.\n\
         Write in clear Markdown paragraphs. No JSON.",
        path.display()
    )
}

pub fn summary_user(doc_name: &str, content: &str) -> String {
    let content = clip_content(content);
    format!(
        "New document: {doc_name}\n\nFull text:\n{content}\n\n\
         Write a summary page for this document in Markdown.\n\n\
         Return a JSON object with two keys:\n\
         - \"description\": A single sentence (under 100 chars) describing the document's main contribution\n\
         - \"content\": The full summary in Markdown. Include key concepts, findings, ideas, \
         and markdown links to concepts that could become cross-document concept pages \
         (e.g. [topic](concepts/topic-slug.md))\n\
         - \"private\": true if the document contains sensitive or personal information \
         (PII, medical/health records, financial/tax data, credentials, legal ID, intimate personal details); \
         false otherwise\n\n\
         Return ONLY valid JSON, no fences."
    )
}

pub fn concepts_plan_user(
    concept_briefs: &str,
    entity_briefs: &str,
    entity_types: &[String],
) -> String {
    let types = entity_types.join(", ");
    format!(
        "Based on the summary above, decide how to update the wiki's CONCEPT pages and ENTITY pages.\n\n\
         A CONCEPT is an abstract, recurring idea/pattern/mechanism. An ENTITY is a specific named thing.\n\n\
         Existing concept pages:\n{concept_briefs}\n\n\
         Existing entity pages:\n{entity_briefs}\n\n\
         Return a JSON object with two top-level keys, \"concepts\" and \"entities\".\n\n\
         \"concepts\" is an object with:\n\
         1. \"create\" — new concepts. Array of {{\"name\": \"concept-slug\", \"title\": \"Title\"}}\n\
         2. \"update\" — existing concepts with significant new info. Same shape.\n\
         3. \"related\" — existing concept slugs to cross-link only. Array of strings.\n\n\
         \"entities\" is an object with the same three keys, but create/update objects \
         add a \"type\" field, one of: {types}.\n\n\
         Rules:\n\
         - For the first few documents, create 2-3 foundational concepts at most.\n\
         - Create an ENTITY page only when central or likely to recur.\n\
         - Prefer \"update\" over \"create\" for existing pages.\n\
         - \"related\" is lightweight cross-linking only.\n\n\
         Return ONLY valid JSON, no fences."
    )
}

pub fn known_targets_user(targets: &str) -> String {
    format!(
        "The wiki currently contains these pages — the COMPLETE list of valid markdown link targets:\n\n\
         {targets}\n\n\
         Rules for cross-links in all subsequent responses:\n\
         - Use `[label](summaries/Y.md)`, `[label](concepts/X.md)`, `[label](entities/Z.md)`.\n\
         - X/Y/Z must appear in the list above.\n\
         - Do NOT invent new link targets; use plain text instead."
    )
}

pub fn concept_page_user(doc_name: &str, title: &str, update: bool) -> String {
    let update_instruction = if update {
        "Integrate new information from the document summary above into this concept."
    } else {
        "Create a new concept page from the document summary above."
    };
    format!(
        "Write the concept page for: {title}\n\n\
         This concept relates to document \"{doc_name}\" summarized above.\n\
         {update_instruction}\n\n\
         Return JSON with keys \"description\" (one sentence) and \"content\" (full Markdown with OKF links).\n\
         Return ONLY valid JSON."
    )
}

pub fn concept_update_user(doc_name: &str, title: &str, existing: &str) -> String {
    format!(
        "Update the concept page for: {title}\n\nCurrent content:\n{existing}\n\n\
         Integrate new information from document \"{doc_name}\" (summarized above). \
         Rewrite the full page — do not just append.\n\n\
         Return JSON with keys \"description\" and \"content\".\n\
         Return ONLY valid JSON."
    )
}

pub fn entity_page_user(doc_name: &str, title: &str, entity_type: &str) -> String {
    format!(
        "Write the entity page for: {title} (type: {entity_type})\n\n\
         This entity relates to document \"{doc_name}\" summarized above.\n\n\
         Return JSON with keys \"description\", \"type\", and \"content\" (full Markdown).\n\
         Return ONLY valid JSON."
    )
}

pub fn entity_update_user(doc_name: &str, title: &str, entity_type: &str, existing: &str) -> String {
    format!(
        "Update the entity page for: {title} (type: {entity_type})\n\nCurrent content:\n{existing}\n\n\
         Integrate new facts from document \"{doc_name}\". Rewrite the full page.\n\n\
         Return JSON with keys \"description\", \"type\", and \"content\".\n\
         Return ONLY valid JSON."
    )
}

pub fn summary_rewrite_user() -> String {
    "Rewrite the summary you wrote above into a final version consistent with concept pages \
     now in the wiki (per the whitelist message).\n\n\
     Preserve factual claims. Fix cross-links per whitelist rules.\n\n\
     Return ONLY the rewritten Markdown (no JSON, no fences)."
        .into()
}

pub fn long_doc_summary_user(doc_name: &str, doc_id: &str, content: &str) -> String {
    format!(
        "This is a PageIndex summary for long document \"{doc_name}\" (doc_id: {doc_id}):\n\n\
         {content}\n\n\
         Based on this structured summary, write a concise overview capturing key themes."
    )
}

pub fn query_user(question: &str, wiki_context: &str) -> String {
    format!(
        "Answer the question using ONLY the wiki context below.\n\
         When you use a fact, cite the wiki page inline (e.g. [paper-name](summaries/paper-name.md)).\n\
         End with a **Sources** section listing every wiki page you used and any \
         `Resource:` paths shown in the context. Omit **Sources** only if the context \
         has nothing relevant.\n\n\
         Wiki context:\n{wiki_context}\n\n\
         Question: {question}"
    )
}

pub fn chat_user(history: &str, wiki_context: &str, question: &str) -> String {
    format!(
        "Conversation history:\n{history}\n\n\
         Answer using ONLY the wiki context below.\n\
         When you use a fact, cite the wiki page inline (e.g. [paper-name](summaries/paper-name.md)).\n\
         End with a **Sources** section listing every wiki page you used and any \
         `Resource:` paths shown in the context. Omit **Sources** only if the context \
         has nothing relevant.\n\n\
         Wiki context:\n{wiki_context}\n\n\
         User: {question}"
    )
}

pub fn tree_select_user(question: &str, trees: &str) -> String {
    format!(
        "Given these document trees, select which sections to read to answer the question.\n\n\
         Trees:\n{trees}\n\n\
         Question: {question}\n\n\
         Return JSON: {{\"selections\": [{{\"doc_name\": \"...\", \"title\": \"section title\"}}]}}\n\
         Return ONLY valid JSON."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_content_respects_utf8_boundaries() {
        let s = "中".repeat(12_001);
        let clipped = clip_content(&s);
        assert!(clipped.contains('中'));
        assert!(clipped.contains("chars omitted"));
        assert!(std::str::from_utf8(clipped.as_bytes()).is_ok());
    }
}
