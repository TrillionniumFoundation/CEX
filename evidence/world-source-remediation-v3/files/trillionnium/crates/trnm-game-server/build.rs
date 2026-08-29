use std::{
    env, fs,
    path::{Path, PathBuf},
};

const LIB_HEADER: &str = r#"#![recursion_limit = "512"]

mod cex;
mod map;
mod operations_v1;
mod product_v2;
mod production_v1;
mod published_tick_journal;
pub mod signer_protocol;
mod stream;

"#;

fn fail(message: impl AsRef<str>) -> ! {
    panic!(
        "WORLD-P0-001 source transform failed closed: {}",
        message.as_ref()
    );
}

fn replace_once(source: &mut String, old: &str, new: &str, label: &str) {
    let count = source.matches(old).count();
    match count {
        1 => *source = source.replacen(old, new, 1),
        0 if source.contains(new) => {}
        _ => fail(format!(
            "{label}: expected one reviewed source shape, found {count}"
        )),
    }
}

fn rewrite_migration_includes(source: &str) -> String {
    let needle = "include_str!(\"../migrations/";
    source
        .lines()
        .map(|line| {
            let Some(start) = line.find(needle) else {
                return line.to_string();
            };
            let path_start = start + needle.len();
            let Some(end_offset) = line[path_start..].find("\");") else {
                fail(format!("malformed migration include: {line}"));
            };
            let path_end = path_start + end_offset;
            let relative = &line[path_start..path_end];
            format!(
                "{}include_str!(concat!(::std::env!(\"CARGO_MANIFEST_DIR\"), \"/migrations/{}\"));{}",
                &line[..start],
                relative,
                &line[path_end + 3..]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn generate_game_server(out_dir: &Path) {
    let template_path = PathBuf::from("src/lib.rs.in");
    let mut source = fs::read_to_string(&template_path)
        .unwrap_or_else(|error| fail(format!("read {}: {error}", template_path.display())));

    replace_once(
        &mut source,
        "const MIGRATION_V15: &str = include_str!(\"../migrations/0015_online_realtime_hot_path_v1.sql\");",
        r#"const MIGRATION_V15: &str = include_str!("../migrations/0015_online_realtime_hot_path_v1.sql");
const MIGRATION_V16: &str = include_str!("../migrations/0016_online_settlement_outbox_v1.sql");
const MIGRATION_V17: &str =
    include_str!("../migrations/0017_online_settlement_worker_runtime_v1.sql");
const MIGRATION_V18: &str =
    include_str!("../migrations/0018_online_settlement_operator_controls_v1.sql");"#,
        "game-server migration constants",
    );
    replace_once(
        &mut source,
        "        (15, \"0015_online_realtime_hot_path_v1\", MIGRATION_V15),\n",
        r#"        (15, "0015_online_realtime_hot_path_v1", MIGRATION_V15),
        (16, "0016_online_settlement_outbox_v1", MIGRATION_V16),
        (
            17,
            "0017_online_settlement_worker_runtime_v1",
            MIGRATION_V17,
        ),
        (
            18,
            "0018_online_settlement_operator_controls_v1",
            MIGRATION_V18,
        ),
"#,
        "game-server migration ledger",
    );

    let legacy_loop = r#"    let settlement_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = settle_pending_matches(&settlement_state, 2).await {
                tracing::error!(%error, "online authority settlement remains pending");
            }
        }
    });

"#;
    if source.contains(legacy_loop) {
        source = source.replacen(legacy_loop, "", 1);
    } else if source.contains("settle_pending_matches(&settlement_state") {
        fail("legacy settlement loop drifted from the reviewed source shape");
    }

    let legacy_signature =
        "pub async fn settle_pending_matches(state: &AppState, limit: i64) -> Result<u64, String> {";
    let fail_closed_signature =
        "pub async fn settle_pending_matches(_state: &AppState, _limit: i64) -> Result<u64, String> {";
    if let Some(start) = source.find(legacy_signature) {
        let end_marker = "\nasync fn persist_campaign(\n";
        let Some(relative_end) = source[start..].find(end_marker) else {
            fail("cannot find reviewed end of legacy settlement function");
        };
        let end = start + relative_end;
        let replacement = r#"/// Compatibility API retained only to fail closed for downstream callers.
///
/// Terminal economic settlement is owned by the independently deployed
/// `trnm-settlement-worker`. The game-server process must never execute signer
/// or CEX I/O, mutate campaign economic queues, or advance the terminal
/// settlement marker itself.
pub async fn settle_pending_matches(_state: &AppState, _limit: i64) -> Result<u64, String> {
    Err(
        "terminal settlement is owned by trnm-settlement-worker; in-process settlement is prohibited"
            .to_string(),
    )
}
"#;
        source.replace_range(start..end, replacement);
    } else if !source.contains(fail_closed_signature) {
        fail("legacy settlement function is neither reviewed legacy nor fail-closed form");
    }

    if source.contains("reconcile_economy(&state.cex") {
        fail("game-server still contains synchronous CEX reconciliation");
    }
    if source.contains("settle_pending_matches(&settlement_state") {
        fail("game-server still schedules in-process settlement");
    }

    let Some(body) = source.strip_prefix(LIB_HEADER) else {
        fail("game-server crate header drifted from the reviewed template");
    };
    let generated = rewrite_migration_includes(body);
    fs::write(out_dir.join("trnm_game_server_lib_generated.rs"), generated)
        .unwrap_or_else(|error| fail(format!("write generated game server: {error}")));
}

fn generate_settlement_worker(out_dir: &Path) {
    let template_path = PathBuf::from("src/settlement_worker.rs.in");
    let mut source = fs::read_to_string(&template_path)
        .unwrap_or_else(|error| fail(format!("read {}: {error}", template_path.display())));
    replace_once(
        &mut source,
        "const MIGRATION_V17: &str = include_str!(\"../migrations/0017_online_settlement_worker_runtime_v1.sql\");",
        r#"const MIGRATION_V17: &str = include_str!("../migrations/0017_online_settlement_worker_runtime_v1.sql");
const MIGRATION_V18: &str =
    include_str!("../migrations/0018_online_settlement_operator_controls_v1.sql");"#,
        "settlement-worker migration constant",
    );
    replace_once(
        &mut source,
        r#"        (
            17_i32,
            "0017_online_settlement_worker_runtime_v1",
            MIGRATION_V17,
        ),
"#,
        r#"        (
            17_i32,
            "0017_online_settlement_worker_runtime_v1",
            MIGRATION_V17,
        ),
        (
            18_i32,
            "0018_online_settlement_operator_controls_v1",
            MIGRATION_V18,
        ),
"#,
        "settlement-worker migration ledger",
    );
    let generated = rewrite_migration_includes(&source);
    fs::write(
        out_dir.join("trnm_settlement_worker_generated.rs"),
        generated,
    )
    .unwrap_or_else(|error| fail(format!("write generated settlement worker: {error}")));
}

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs.in");
    println!("cargo:rerun-if-changed=src/settlement_worker.rs.in");
    println!("cargo:rerun-if-changed=migrations");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is required"));
    generate_game_server(&out_dir);
    generate_settlement_worker(&out_dir);
}
