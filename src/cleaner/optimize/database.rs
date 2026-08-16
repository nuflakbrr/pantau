use std::path::PathBuf;
use std::process::Command;

pub fn optimize_sqlite_databases(dry_run: bool) -> (usize, Vec<String>) {
    let mut count = 0;
    let mut logs = Vec::new();
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));

    let db_targets = vec![
        home.join("Library/Safari/History.db"),
        home.join("Library/Mail/V10/MailData/Envelope Index"),
        home.join("Library/Mail/V9/MailData/Envelope Index"),
        home.join("Library/Group Containers/group.com.apple.notes/NoteStore.sqlite"),
    ];

    for db_path in db_targets {
        if db_path.exists() {
            if dry_run {
                logs.push(format!("[DRY RUN] Would vacuum SQLite database: {}", db_path.display()));
                count += 1;
            } else {
                let res = Command::new("sqlite3")
                    .arg(&db_path)
                    .arg("VACUUM;")
                    .output();
                if res.is_ok() {
                    logs.push(format!("✓ Vacuumed database: {}", db_path.display()));
                    count += 1;
                }
            }
        }
    }

    (count, logs)
}
