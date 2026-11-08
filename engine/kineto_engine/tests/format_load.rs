use kineto_engine::format;

const SAMPLE_KVK: &[u8] = include_bytes!("fixtures/sample.kvk");

#[test]
fn sample_kvk_loads_and_has_expected_structure() {
    let doc = format::load(SAMPLE_KVK).expect("failed to parse sample.kvk");

    println!(
        "Document: \"{}\" {}x{} @ {}fps",
        doc.name(),
        doc.canvas_width(),
        doc.canvas_height(),
        doc.frame_rate()
    );

    let nodes = doc.nodes().unwrap_or_default();
    println!("  {} node(s)", nodes.len());
    for node in &nodes {
        let kind = match node.component_type() {
            format::generated::kineto::Component::Shape => "Shape",
            format::generated::kineto::Component::Path => "Path",
            format::generated::kineto::Component::Group => "Group",
            format::generated::kineto::Component::Text => "Text",
            format::generated::kineto::Component::Image => "Image",
            format::generated::kineto::Component::CompoundPath => "CompoundPath",
            _ => "?",
        };
        println!(
            "  - \"{}\" ({}) visible={} opacity={}",
            node.name(),
            kind,
            node.visible(),
            node.opacity()
        );
    }

    assert!(!doc.name().is_empty());
    assert!(doc.canvas_width() > 0.0);
    assert!(doc.canvas_height() > 0.0);
    assert!(doc.frame_rate() > 0);
}

#[test]
fn sample_kvk_animations_accessible() {
    let doc = format::load(SAMPLE_KVK).expect("failed to parse sample.kvk");
    let animations = doc.animations().unwrap_or_default();
    println!("  {} animation(s)", animations.len());
    for anim in &animations {
        println!(
            "  - \"{}\" duration={}s tracks={}",
            anim.name(),
            anim.duration(),
            anim.tracks().map(|t| t.len()).unwrap_or(0)
        );
    }
}
