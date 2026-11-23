//! Knowledge is compiled from the very files shipped as the public skill.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const GUIDE_VERSION: &str = "2026-11-17.2";
const ENTRY: &str = include_str!("../../../skills/s33d-modeling/SKILL.md");
const STRATEGY: &str =
    include_str!("../../../skills/s33d-modeling/references/modeling-strategy.md");
const MODEL: &str = include_str!("../../../skills/s33d-modeling/references/modeling-patterns.md");
const PHOTO: &str =
    include_str!("../../../skills/s33d-modeling/references/photo-reconstruction.md");
const VISUAL: &str = include_str!("../../../skills/s33d-modeling/references/visual-validation.md");
const CONTRACTS: &str =
    include_str!("../../../skills/s33d-modeling/references/interface-contracts.md");

pub fn guide(mode: &str) -> Result<Value, String> {
    let mut references = vec![("modeling-strategy", STRATEGY)];
    references.extend(match mode {
        "auto" | "photo" => vec![
            ("photo-reconstruction", PHOTO),
            ("visual-validation", VISUAL),
        ],
        "model" | "edit" => vec![("modeling-patterns", MODEL), ("visual-validation", VISUAL)],
        "geometry" => vec![("interface-contracts", CONTRACTS)],
        _ => return Err("mode must be auto, photo, model, edit or geometry".into()),
    });
    let sources = sources();
    Ok(json!({
        "guide_version": GUIDE_VERSION, "application_version": env!("CARGO_PKG_VERSION"),
        "content_sha256": format!("{:x}", Sha256::digest(sources.iter().map(|(_,text)|*text).collect::<Vec<_>>().join("\n").as_bytes())),
        "source_manifest":sources.iter().map(|(path,content)|json!({"path":path,"sha256":format!("{:x}",Sha256::digest(content.as_bytes()))})).collect::<Vec<_>>(),
        "execution_state_template":{
            "maintained_by":"executing Agent in its conversation/checkpoint; not persisted or evaluated by S33D",
            "target_scale_scope":null,"evidence_unknowns":[],"stage_gate":null,"shared_controls":[],
            "verified_bodies":[],"largest_deviation_cause":null,"decision_next_operation":null,"required_rechecks":[],
            "acceptance":{"appearance":"unverified","geometry":"unverified","dimensions_function":"unverified","editability":"unverified","save_reopen":"unverified"},
            "blockers_simplifications":[]
        },
        "mode":mode, "entry":ENTRY,
        "references":references.into_iter().map(|(name,content)|json!({"name":name,"content":content})).collect::<Vec<_>>(),
        "start":"Read the live catalog and document.summary. You, the visual Agent, interpret photos already visible in your chat, construct using published S33D methods, compare s33d_render images with the photos, correct and save to the requested destination. No S33D import, asset ID, server-readable photo path or second upload is required for chat photos you can see. reference.import and s33d_reference are optional project reference storage or alternate image delivery; only that route needs a real available local path or imported asset.",
        "limits":"A visual Agent and supported MCP image transport are required. This tool supplies knowledge and provenance, not a server-maintained plan, an automatic likeness verdict or proof of Agent compliance. No photogrammetry, guaranteed manufacturing geometry, or cross-restart exactly-once workflow recovery."
    }))
}

fn sources() -> [(&'static str, &'static str); 6] {
    [
        ("skills/s33d-modeling/SKILL.md", ENTRY),
        (
            "skills/s33d-modeling/references/modeling-strategy.md",
            STRATEGY,
        ),
        (
            "skills/s33d-modeling/references/modeling-patterns.md",
            MODEL,
        ),
        (
            "skills/s33d-modeling/references/photo-reconstruction.md",
            PHOTO,
        ),
        (
            "skills/s33d-modeling/references/visual-validation.md",
            VISUAL,
        ),
        (
            "skills/s33d-modeling/references/interface-contracts.md",
            CONTRACTS,
        ),
    ]
}

pub fn resources() -> Value {
    json!({"resources":[{"uri":"s33d://workflow/modeling","name":"S33D modeling workflow","mimeType":"text/markdown","description":"Version-matched text, photo and edit workflow; also callable with s33d_workflow."}]})
}

pub fn resource(uri: &str) -> Result<Value, String> {
    if uri != "s33d://workflow/modeling" {
        return Err("Unknown S33D resource".into());
    }
    Ok(
        json!({"contents":[{"uri":uri,"mimeType":"text/markdown","text":(sources().iter().map(|(_,text)|*text).collect::<Vec<_>>().join("\n\n"))}]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_and_resource_share_the_exact_strategy_and_source_hashes() {
        let baseline = guide("auto").unwrap();
        for mode in ["auto", "photo", "model", "edit", "geometry"] {
            let value = guide(mode).unwrap();
            assert_eq!(value["content_sha256"], baseline["content_sha256"]);
            assert_eq!(value["references"][0]["content"], STRATEGY);
            assert_eq!(
                value["source_manifest"].as_array().unwrap().len(),
                sources().len()
            );
            for (i, (path, content)) in sources().iter().enumerate() {
                assert_eq!(value["source_manifest"][i]["path"], *path);
                assert_eq!(
                    value["source_manifest"][i]["sha256"],
                    format!("{:x}", Sha256::digest(content.as_bytes()))
                );
            }
        }
        assert!(guide("unknown").is_err());
        let resource = resource("s33d://workflow/modeling").unwrap();
        assert_eq!(
            resource["contents"][0]["text"],
            sources()
                .iter()
                .map(|(_, text)| *text)
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }
}
