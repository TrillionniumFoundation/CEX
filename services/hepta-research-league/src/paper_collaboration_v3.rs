#![expect(
    unused_imports,
    reason = "the exact source body imports base64::Engine locally while paper_raid_v2 already supplies the trait; body identity is machine-locked"
)]

include!("paper_collaboration_v3_body.rs");
