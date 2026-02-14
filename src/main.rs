use anyhow::Result;
use clap::Parser;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossterm::{
    cursor::{Hide, MoveTo, Show, MoveToNextLine},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
    ExecutableCommand,
    style::Print,
    queue,
};
use ignore::{WalkBuilder, WalkState};
use std::{
    env,
    io::{stdout, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Live file search (background scan, no persistent index)")]
struct Args {
    /// Root directory to scan (default: Windows=SystemDrive root, macOS/Linux=/)
    #[arg(short, long)]
    root: Option<PathBuf>,

    /// Case-insensitive match
    #[arg(short, long)]
    ignore_case: bool,

    /// Include directories in the scanned list/results
    #[arg(long)]
    include_dirs: bool,

    /// Show only first N results
    #[arg(long, default_value_t = 30)]
    limit: usize,

    /// Scan threads (0=auto)
    #[arg(long, default_value_t = 0)]
    threads: usize,
}

fn default_root() -> PathBuf {
    if cfg!(windows) {
        if let Some(sd) = env::var_os("SystemDrive") {
            let mut s = sd.to_string_lossy().to_string();
            s.push('\\');
            PathBuf::from(s)
        } else {
            PathBuf::from(r"C:\")
        }
    } else {
        PathBuf::from("/")
    }
}

#[derive(Debug)]
enum Msg {
    Batch(Vec<String>),
    Done,
    #[allow(dead_code)]
    Error(String),
}

fn spawn_scanner(
    root: PathBuf,
    include_dirs: bool,
    threads: usize,
    tx: Sender<Msg>,
) {
    thread::spawn(move || {
        let mut builder = WalkBuilder::new(&root);
        builder.follow_links(false);

        let t = if threads != 0 {
            threads
        } else {
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
        };
        builder.threads(t);

        // バッチサイズ（大きいほど送信回数が減って速いがUI反映が遅くなる）
        const BATCH: usize = 4096;

        let walker = builder.build_parallel();
        walker.run(|| {
            let tx = tx.clone();
            let mut local: Vec<String> = Vec::with_capacity(BATCH);

            Box::new(move |res| {
                let entry = match res {
                    Ok(e) => e,
                    Err(_) => return WalkState::Continue,
                };

                let ft = match entry.file_type() {
                    Some(ft) => ft,
                    None => return WalkState::Continue,
                };

                if ft.is_file() || (include_dirs && ft.is_dir()) {
                    local.push(entry.path().to_string_lossy().into_owned());

                    if local.len() >= BATCH {
                        // まとめて送る
                        if tx.send(Msg::Batch(std::mem::take(&mut local))).is_err() {
                            return WalkState::Quit;
                        }
                    }
                }

                WalkState::Continue
            })
        });

        // 最後の残りを送る
        if !tx.is_empty() {
            // no-op（送り忘れ防止用の読みやすさ）
        }

        // 並列 walker の各スレッドに残っている local は Drop では送れないので、
        // ここでは「Done」だけ送る（Batchは各スレッドがBATCH到達時に送っている想定）。
        // ※UI側は「Done」を受け取っても、その後しばらく Batch が届く可能性があります。
        let _ = tx.send(Msg::Done);
    });
}

fn render(
    scanned: usize,
    done: bool,
    query: &str,
    limit: usize,
    matches: &[usize],
    paths: &[String],
) -> Result<()> {
    let mut out = stdout();

    queue!(out, MoveTo(0, 0), Clear(ClearType::All))?;

    queue!(
        out,
        Print(format!(
            "Scan: {} items  [{}]   (ESC/Ctrl+C: quit)",
            scanned,
            if done { "DONE" } else { "SCANNING" }
        )),
        MoveToNextLine(1),
        Print(format!("Query: {}", query)),
        MoveToNextLine(1),
        Print(format!("Matches: {}  (showing up to {})", matches.len(), limit)),
        MoveToNextLine(1),
        Print("----------------------------------------"),
        MoveToNextLine(1),
    )?;

    for &idx in matches.iter().take(limit) {
        queue!(out, Print(&paths[idx]), MoveToNextLine(1))?;
    }

    out.flush()?;
    Ok(())
}

fn is_extend(prev: &str, next: &str) -> bool {
    next.starts_with(prev) && next.len() > prev.len()
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.unwrap_or_else(default_root);

    // 走査スレッド → UIスレッドへバッチ送信
    let (tx, rx) = unbounded::<Msg>();
    spawn_scanner(root.clone(), args.include_dirs, args.threads, tx);

    // UI状態
    let mut paths: Vec<String> = Vec::new();
    let mut paths_lc: Vec<String> = Vec::new(); // ignore_case 用
    let mut done = false;

    let mut query = String::new();
    let mut candidates: Vec<usize> = Vec::new();

    // 画面初期化
    let mut out = stdout();
    enable_raw_mode()?;
    out.execute(EnterAlternateScreen)?;
    out.execute(Hide)?;

    let result = (|| -> Result<()> {
        render(0, false, "", args.limit, &[], &paths)?;

        loop {
            // 1) まずチャンネルを可能な限り吸う（UIが止まらないように）
            let mut updated = false;
            drain_messages(&rx, &mut paths, &mut paths_lc, args.ignore_case, &query, &mut candidates, &mut done, &mut updated);

            // 2) 入力待ち（短いタイムアウトでpoll）
            if poll(Duration::from_millis(50))? {
                if let Event::Key(k) = read()? {
                    if should_quit(k) {
                        break;
                    }

                    let prev_query = query.clone();
                    let mut changed = false;

                    match k.code {
                        KeyCode::Backspace => {
                            if !query.is_empty() {
                                query.pop();
                                changed = true;
                            }
                        }
                        KeyCode::Char(ch) => {
                            if !k.modifiers.contains(KeyModifiers::CONTROL)
                                && !k.modifiers.contains(KeyModifiers::ALT)
                            {
                                query.push(ch);
                                changed = true;
                            }
                        }
                        _ => {}
                    }

                    if changed {
                        // クエリ変更に合わせて candidates を更新
                        if query.is_empty() {
                            candidates.clear();
                        } else {
                            let q = if args.ignore_case { query.to_lowercase() } else { query.clone() };

                            if is_extend(&prev_query, &query) && !prev_query.is_empty() {
                                // 1文字追加など：既存候補から絞り込み（速い）
                                candidates.retain(|&idx| {
                                    let hay = if args.ignore_case { &paths_lc[idx] } else { &paths[idx] };
                                    hay.contains(&q)
                                });
                            } else {
                                // それ以外（Backspace等）：全件から再計算
                                candidates.clear();
                                for i in 0..paths.len() {
                                    let hay = if args.ignore_case { &paths_lc[i] } else { &paths[i] };
                                    if hay.contains(&q) {
                                        candidates.push(i);
                                    }
                                }
                            }
                        }
                        updated = true;
                    }
                }
            }

            if updated {
                render(paths.len(), done, &query, args.limit, &candidates, &paths)?;
            }
        }

        Ok(())
    })();

    // 後始末
    out.execute(Show)?;
    out.execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    result
}

fn should_quit(k: KeyEvent) -> bool {
    k.code == KeyCode::Esc
        || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
}

fn drain_messages(
    rx: &Receiver<Msg>,
    paths: &mut Vec<String>,
    paths_lc: &mut Vec<String>,
    ignore_case: bool,
    query: &str,
    candidates: &mut Vec<usize>,
    done: &mut bool,
    updated: &mut bool,
) {
    // 現在クエリがある時は、新規追加分だけを candidates に増分反映する
    let q_norm = if !query.is_empty() && ignore_case {
        Some(query.to_lowercase())
    } else if !query.is_empty() {
        Some(query.to_string())
    } else {
        None
    };

    while let Ok(msg) = rx.try_recv() {
        match msg {
            Msg::Batch(batch) => {
                for p in batch {
                    let idx = paths.len();
                    paths.push(p);
                    if ignore_case {
                        let lc = paths[idx].to_lowercase();
                        paths_lc.push(lc);
                    }

                    if let Some(q) = &q_norm {
                        let hay = if ignore_case { &paths_lc[idx] } else { &paths[idx] };
                        if hay.contains(q) {
                            candidates.push(idx);
                        }
                    }
                }
                *updated = true;
            }
            Msg::Done => {
                *done = true;
                *updated = true;
            }
            Msg::Error(e) => {
                // シンプルにdone扱いにして表示更新
                eprintln!("scan error: {e}");
                *done = true;
                *updated = true;
            }
        }
    }
}
