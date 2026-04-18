/// Cleanup logic: remove old DB records and their associated physical files.
///
/// Deletion order matters because of foreign-key relationships:
///   frames → ocr_text (frame_id FK)
///   frames → video_chunks (frame_id FK) — actually video_chunks may reference frames
///   audio_chunks → audio_transcriptions (audio_chunk_id FK)
///
/// We delete child rows first to avoid FK violations, even if the DB was
/// compiled without FK enforcement (which is the default for SQLite).
use crate::{config::Config, filter, storage};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, Result as SqlResult};
use std::path::Path;

/// Summary of everything that was deleted (or would be in dry-run mode).
#[derive(Debug, Default)]
pub struct CleanupSummary {
    pub frames_deleted: usize,
    pub audio_chunks_deleted: usize,
    pub audio_transcriptions_deleted: usize,
    pub ocr_text_deleted: usize,
    pub video_chunks_deleted: usize,
    pub files_deleted: usize,
    pub files_missing: usize,
}

impl CleanupSummary {
    fn print(&self, dry_run: bool) {
        let label = if dry_run { "Would delete" } else { "Deleted" };
        println!("\n=== Cleanup Summary ===");
        println!("  {label} {:<6} frame records", self.frames_deleted);
        println!("  {label} {:<6} audio_chunk records", self.audio_chunks_deleted);
        println!("  {label} {:<6} audio_transcription records", self.audio_transcriptions_deleted);
        println!("  {label} {:<6} ocr_text records", self.ocr_text_deleted);
        println!("  {label} {:<6} video_chunk records", self.video_chunks_deleted);
        println!("  {label} {:<6} physical files", self.files_deleted);
        if self.files_missing > 0 {
            println!("  {} files were already absent on disk", self.files_missing);
        }
    }
}

/// Entry point called from `main`.
pub fn run(
    cfg: &Config,
    dry_run: bool,
    days_override: Option<u32>,
) -> Result<(), Box<dyn std::error::Error>> {
    let retention = days_override.unwrap_or(cfg.retention_days);
    let cutoff: DateTime<Utc> = Utc::now() - Duration::days(retention as i64);

    println!(
        "Retention: {} days  |  Cutoff: {}",
        retention,
        cutoff.to_rfc3339()
    );
    if dry_run {
        println!("[DRY RUN] No changes will be made.");
    }

    let db_path = cfg.resolved_data_dir().join("db.sqlite");
    if !db_path.exists() {
        println!("No database found at {}. Nothing to do.", db_path.display());
        return Ok(());
    }

    let conn = Connection::open(&db_path)?;
    let mut summary = CleanupSummary::default();

    // Step 1: collect IDs and file paths for stale or blacklisted frames.
    let frame_targets = collect_frame_targets(&conn, &cutoff, cfg)?;

    // Step 2: collect IDs and file paths for stale or blacklisted audio chunks.
    let audio_targets = collect_audio_targets(&conn, &cutoff, cfg)?;

    // Step 3: delete child rows, then parent rows, then physical files.
    process_frames(&conn, &frame_targets, dry_run, &mut summary)?;
    process_audio(&conn, &audio_targets, dry_run, &mut summary)?;

    // Step 4: delete physical files collected during frame/audio processing.
    delete_files(&frame_targets.file_paths, dry_run, &mut summary);
    delete_files(&audio_targets.file_paths, dry_run, &mut summary);

    summary.print(dry_run);
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct FrameTargets {
    ids: Vec<i64>,
    file_paths: Vec<String>,
}

struct AudioTargets {
    ids: Vec<i64>,
    file_paths: Vec<String>,
}

// ---------------------------------------------------------------------------
// Collection helpers
// ---------------------------------------------------------------------------

/// Query frames that are either older than `cutoff` or belong to a blacklisted app.
fn collect_frame_targets(
    conn: &Connection,
    cutoff: &DateTime<Utc>,
    cfg: &Config,
) -> SqlResult<FrameTargets> {
    // We read *all* rows that could be candidates and then apply the blacklist
    // filter in Rust, because SQLite doesn't know about our blacklist logic.
    let cutoff_str = cutoff.to_rfc3339();

    let mut stmt = conn.prepare(
        "SELECT id, snapshot_path, app_name FROM frames WHERE timestamp < ?1",
    )?;

    let rows = stmt.query_map([&cutoff_str], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(),
        ))
    })?;

    // Also collect frames from blacklisted apps regardless of timestamp.
    let mut ids = Vec::new();
    let mut file_paths = Vec::new();

    for row in rows {
        let (id, path, _app) = row?;
        ids.push(id);
        file_paths.push(path);
    }

    // Extra pass: frames for apps that fail the filter policy (blacklisted or
    // not whitelisted) and are within the retention window.
    if !cfg.blacklist.is_empty() || !cfg.whitelist.is_empty() {
        collect_filtered_frames(conn, cfg, &mut ids, &mut file_paths)?;
    }

    // Deduplicate in case a frame qualifies on both criteria.
    dedup_by_id(&mut ids, &mut file_paths);

    Ok(FrameTargets { ids, file_paths })
}

/// Append frames that fail the filter policy (blacklisted or not whitelisted).
fn collect_filtered_frames(
    conn: &Connection,
    cfg: &Config,
    ids: &mut Vec<i64>,
    file_paths: &mut Vec<String>,
) -> SqlResult<()> {
    let existing_ids: std::collections::HashSet<i64> = ids.iter().copied().collect();

    let mut stmt =
        conn.prepare("SELECT id, snapshot_path, app_name FROM frames WHERE app_name IS NOT NULL")?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, String>(2).unwrap_or_default(),
        ))
    })?;

    for row in rows {
        let (id, path, app) = row?;
        if existing_ids.contains(&id) {
            continue;
        }
        // Use should_keep so both blacklist and whitelist policies are applied.
        if !filter::should_keep(&app, cfg) {
            ids.push(id);
            file_paths.push(path);
        }
    }

    Ok(())
}

/// Query audio chunks that are either older than `cutoff` or belong to a blacklisted app.
fn collect_audio_targets(
    conn: &Connection,
    cutoff: &DateTime<Utc>,
    cfg: &Config,
) -> SqlResult<AudioTargets> {
    let cutoff_str = cutoff.to_rfc3339();

    let mut stmt = conn.prepare(
        "SELECT id, file_path FROM audio_chunks WHERE timestamp < ?1",
    )?;

    let rows = stmt.query_map([&cutoff_str], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1).unwrap_or_default()))
    })?;

    let mut ids = Vec::new();
    let mut file_paths = Vec::new();

    for row in rows {
        let (id, path) = row?;
        ids.push(id);
        file_paths.push(path);
    }

    // audio_chunks may not have an app_name column; blacklist by app is handled
    // at the frame level for screen recordings. Nothing extra needed here.
    let _ = cfg; // silence unused warning if blacklist is empty

    dedup_by_id(&mut ids, &mut file_paths);

    Ok(AudioTargets { ids, file_paths })
}

// ---------------------------------------------------------------------------
// Deletion helpers
// ---------------------------------------------------------------------------

/// Delete ocr_text, video_chunks (child rows), then frames (parent rows).
fn process_frames(
    conn: &Connection,
    targets: &FrameTargets,
    dry_run: bool,
    summary: &mut CleanupSummary,
) -> SqlResult<()> {
    if targets.ids.is_empty() {
        return Ok(());
    }

    let id_list = targets.ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");

    if !dry_run {
        let ocr_deleted = conn.execute(
            &format!("DELETE FROM ocr_text WHERE frame_id IN ({id_list})"),
            [],
        )?;
        summary.ocr_text_deleted += ocr_deleted;

        let vc_deleted = conn.execute(
            &format!("DELETE FROM video_chunks WHERE frame_id IN ({id_list})"),
            [],
        )?;
        summary.video_chunks_deleted += vc_deleted;

        let f_deleted =
            conn.execute(&format!("DELETE FROM frames WHERE id IN ({id_list})"), [])?;
        summary.frames_deleted += f_deleted;
    } else {
        // Dry-run: count what would be deleted.
        summary.ocr_text_deleted +=
            count_matching(conn, "ocr_text", "frame_id", &targets.ids);
        summary.video_chunks_deleted +=
            count_matching(conn, "video_chunks", "frame_id", &targets.ids);
        summary.frames_deleted += targets.ids.len();
    }

    Ok(())
}

/// Delete audio_transcriptions (child rows), then audio_chunks (parent rows).
fn process_audio(
    conn: &Connection,
    targets: &AudioTargets,
    dry_run: bool,
    summary: &mut CleanupSummary,
) -> SqlResult<()> {
    if targets.ids.is_empty() {
        return Ok(());
    }

    let id_list = targets.ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");

    if !dry_run {
        let at_deleted = conn.execute(
            &format!("DELETE FROM audio_transcriptions WHERE audio_chunk_id IN ({id_list})"),
            [],
        )?;
        summary.audio_transcriptions_deleted += at_deleted;

        let ac_deleted = conn.execute(
            &format!("DELETE FROM audio_chunks WHERE id IN ({id_list})"),
            [],
        )?;
        summary.audio_chunks_deleted += ac_deleted;
    } else {
        summary.audio_transcriptions_deleted +=
            count_matching(conn, "audio_transcriptions", "audio_chunk_id", &targets.ids);
        summary.audio_chunks_deleted += targets.ids.len();
    }

    Ok(())
}

/// Delete physical files and update summary counters.
fn delete_files(paths: &[String], dry_run: bool, summary: &mut CleanupSummary) {
    for path_str in paths {
        if path_str.is_empty() {
            continue;
        }
        let path = Path::new(path_str);
        if dry_run {
            if path.exists() {
                summary.files_deleted += 1;
            } else {
                summary.files_missing += 1;
            }
        } else {
            match storage::delete_file(path) {
                Ok(true) => summary.files_deleted += 1,
                Ok(false) => summary.files_missing += 1,
                Err(e) => eprintln!("  Warning: could not delete {}: {e}", path.display()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/// Count rows in `table` where `column IN ids` without deleting anything.
fn count_matching(conn: &Connection, table: &str, column: &str, ids: &[i64]) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE {column} IN ({id_list})"),
        [],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0) as usize
}

/// Remove duplicate (id, path) pairs, keeping the first occurrence.
fn dedup_by_id(ids: &mut Vec<i64>, paths: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    let mut keep = Vec::new();
    for (id, path) in ids.iter().zip(paths.iter()) {
        if seen.insert(*id) {
            keep.push((*id, path.clone()));
        }
    }
    *ids = keep.iter().map(|(id, _)| *id).collect();
    *paths = keep.into_iter().map(|(_, p)| p).collect();
}
